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
