use super::{ align_up, Locked };
use alloc::alloc::{ GlobalAlloc, Layout };
use core::ptr;

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
    allocations: usize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
            allocations: 0,
        }
    }

    /// 与えられたヒープ領域でバンプアロケータを初期化
    /// このメソッドは一度しか呼ばれてはならない
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start + heap_size;
        self.next = heap_start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAP_SIZE: usize = 256;

    #[repr(align(64))]
    struct TestHeap([u8; HEAP_SIZE]);

    fn heap_bounds(heap: &mut TestHeap) -> (usize, usize) {
        (heap.0.as_mut_ptr() as usize, heap.0.len())
    }

    #[test_case]
    fn bump_allocator_returns_aligned_addresses() {
        let mut heap = TestHeap([0; HEAP_SIZE]);
        let (heap_start, heap_size) = heap_bounds(&mut heap);
        let allocator = Locked::new(BumpAllocator::new());

        unsafe {
            allocator.lock().init(heap_start, heap_size);
        }

        let first_layout = Layout::from_size_align(1, 1).unwrap();
        let second_layout = Layout::from_size_align(8, 64).unwrap();
        let first = unsafe { allocator.alloc(first_layout) };
        let second = unsafe { allocator.alloc(second_layout) };

        assert!(!first.is_null());
        assert!(!second.is_null());
        assert_eq!(second as usize % 64, 0);
        assert!(second as usize >= first as usize + first_layout.size());
    }

    #[test_case]
    fn bump_allocator_returns_null_when_heap_is_exhausted() {
        let mut heap = TestHeap([0; HEAP_SIZE]);
        let (heap_start, heap_size) = heap_bounds(&mut heap);
        let allocator = Locked::new(BumpAllocator::new());

        unsafe {
            allocator.lock().init(heap_start, heap_size);
        }

        let layout = Layout::from_size_align(HEAP_SIZE + 1, 1).unwrap();
        let ptr = unsafe { allocator.alloc(layout) };

        assert!(ptr.is_null());
    }

    #[test_case]
    fn bump_allocator_resets_after_all_allocations_are_deallocated() {
        let mut heap = TestHeap([0; HEAP_SIZE]);
        let (heap_start, heap_size) = heap_bounds(&mut heap);
        let allocator = Locked::new(BumpAllocator::new());

        unsafe {
            allocator.lock().init(heap_start, heap_size);
        }

        let layout = Layout::from_size_align(16, 8).unwrap();
        let first = unsafe { allocator.alloc(layout) };
        let second = unsafe { allocator.alloc(layout) };
        unsafe {
            allocator.dealloc(second, layout);
            allocator.dealloc(first, layout);
        }
        let after_reset = unsafe { allocator.alloc(layout) };

        assert_eq!(after_reset, first);
    }
}

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // 可変参照を得る
        let mut bump = self.lock();

        // 割当開始アドレス: self.next
        let alloc_start = align_up(bump.next, layout.align());
        // 割当終端アドレス: alloc_start + layout.size
        // 足りない場合 null
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(),
        };

        if alloc_end > bump.heap_end {
            // 足りない場合 null
            ptr::null_mut()
        }
        else {
            // カウンタを増やす
            bump.next = alloc_end;
            bump.allocations += 1;
            alloc_start as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // 可変参照を得る
        let mut bump = self.lock();

        // カウンタを減らす
        bump.allocations -= 1;
        
        // 0 になったら、その割当はすべて解放された -> heap_start にリセット
        if bump.allocations == 0 {
            bump.next = bump.heap_start;
        }
    }
}
