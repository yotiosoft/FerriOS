use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

use crate::allocator::fixed_size_block::FixedSizeBlockAllocator;
use crate::libbackend::lock;

pub mod fixed_size_block;
#[cfg(test)]
mod bump;
#[cfg(test)]
mod linked_list;

#[cfg(test)]
pub use crate::libbackend::lock::Locked;

#[global_allocator]
static ALLOCATOR: lock::Locked<FixedSizeBlockAllocator> = lock::Locked::new(FixedSizeBlockAllocator::new());

pub const HEAP_START: usize = 0x_FFFF_8888_0000_0000;
pub const HEAP_SIZE: usize = 1024 * 1024;        // 1MB

pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

pub fn init_heap(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        // ページにマップする物理アドレスを割り当て
        let frame = frame_allocator.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?;
        // PRESENT flag と WRITABLE flag を設定
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        // ページテーブルへの対応付け, flush で TLB 更新
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush()
        };
    }

    // allocator の初期化
    unsafe {
        // lock() で排他制御を得て init() でヒープの境界を引数として呼ぶ
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn align_up_returns_same_address_when_already_aligned() {
        assert_eq!(align_up(0x1000, 8), 0x1000);
        assert_eq!(align_up(0x1010, 16), 0x1010);
    }

    #[test_case]
    fn align_up_rounds_to_next_alignment_boundary() {
        assert_eq!(align_up(0x1001, 8), 0x1008);
        assert_eq!(align_up(0x100f, 16), 0x1010);
        assert_eq!(align_up(0x1011, 64), 0x1040);
    }
}
