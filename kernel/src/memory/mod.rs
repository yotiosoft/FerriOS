use x86_64::registers::control::Cr3Flags;
use x86_64::{ VirtAddr, PhysAddr };
use x86_64::structures::paging::{ FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, page_table::PageTableEntry };
use bootloader_api::info::{ MemoryRegions, MemoryRegionKind };
use spin::Mutex;
use lazy_static::lazy_static;
use crate::thread;

pub mod kmem;
pub mod umem;
pub mod va;

lazy_static! {
    pub static ref KERNEL_PAGE_TABLE_FRAME: Mutex<Option<PhysFrame>> = Mutex::new(None);
    pub static ref PHYSICAL_MEMORY_OFFSET: Mutex<Option<VirtAddr>> = Mutex::new(None);
    pub static ref FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);
}

const PAGETABLE_USER_SPACE_START: usize = 0;
const PAGETABLE_USER_SPACE_END: usize = 256; // PML4 entries 0-255 are for user space
const PAGETABLE_KERNEL_SPACE_START: usize = 256;
const PAGETABLE_KERNEL_SPACE_END: usize = 512;

pub const PHYSICAL_KERNEL_BASE: u64 = 0xFFFF_8000_0000_0000;

pub const PAGE_SIZE: usize = 4096;

pub const STACK_PAGES: usize = 8;
pub const STACK_SIZE: usize = PAGE_SIZE * STACK_PAGES;

/// 新しい OffsetPageTable を初期化する
pub unsafe fn init(physical_memory_offset: VirtAddr, memory_regions: &'static MemoryRegions) -> OffsetPageTable<'static> {
    // カーネルページテーブルアドレスを取得
    let (kernel_frame, _) = x86_64::registers::control::Cr3::read();
    *KERNEL_PAGE_TABLE_FRAME.lock() = Some(kernel_frame);

    // 物理メモリオフセットを取得
    *PHYSICAL_MEMORY_OFFSET.lock() = Some(physical_memory_offset);
    
    // FRAME_ALLOCATOR の初期化
    *FRAME_ALLOCATOR.lock() = Some(unsafe {
        BootInfoFrameAllocator::init(memory_regions)
    });

    // PML4 への可変参照を取得し、mapper を返す
    unsafe {
        let level_4_table = active_level_4_table(physical_memory_offset);
        OffsetPageTable::new(level_4_table, physical_memory_offset)
    }
}

/// 与えられたページをフレーム 0xb8000 に試しにマップする
pub fn create_example_mapping(page: Page, mapper: &mut OffsetPageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>) {
    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    let map_to_result = unsafe {
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}

/// FrameAllcoator
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

/// ブートローダのメモリマップから使用可能なフレームを返す
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    next: usize,
}
impl BootInfoFrameAllocator {
    /// 渡されたメモリマップから FrameAllocator を作る
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// メモリマップによって指定された利用可能なフレームのイテレータを返す
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // メモリマップから利用可能な領域を得る
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.kind == MemoryRegionKind::Usable);
        // それぞれの領域をアドレス範囲に map で変換する
        let addr_ranges = usable_regions.map(|r| r.start..r.end);
        // フレームの開始アドレスのイテレータへと変換する
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // 開始アドレスから PhysFrame 型を得る
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

/// 有効な level4 テーブルへの可変参照を渡す
/// この関数は unsafe であり、一度しか呼び出してはならない
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_str: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_str
}

