use x86_64::registers::control::Cr3Flags;
use x86_64::{ VirtAddr, PhysAddr };
use x86_64::structures::paging::{ FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, page_table::PageTableEntry };
use bootloader_api::info::{ MemoryRegions, MemoryRegionKind };
use spin::Mutex;
use lazy_static::lazy_static;
use crate::thread;

pub mod kmem;
pub mod umem;

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

const PGSIZE: usize = 4096;

fn pml4_index(va: VirtAddr) -> usize { (va.as_u64() as usize >> 39) & 0x1FF }
fn pdpt_index(va: VirtAddr) -> usize { (va.as_u64() as usize >> 30) & 0x1FF }
fn pd_index  (va: VirtAddr) -> usize { (va.as_u64() as usize >> 21) & 0x1FF }
fn pt_index  (va: VirtAddr) -> usize { (va.as_u64() as usize >> 12) & 0x1FF }

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

/// 有効な level4 テーブルへの可変参照を渡す
fn translate_addr_inner(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    use x86_64::structures::paging::page_table::FrameError;
    use x86_64::registers::control::Cr3;

    // 有効な level4 フレームを読み込み
    let (level_4_table_frame, _) = Cr3::read();

    let table_indexes = [
        addr.p4_index(), addr.p3_index(), addr.p2_index(), addr.p1_index()
    ];
    let mut frame = level_4_table_frame;

    // pagetable walk
    for &index in &table_indexes {
        let virt = physical_memory_offset + frame.start_address().as_u64();
        let table_ptr: *const PageTable = virt.as_ptr();
        let table = unsafe { &*table_ptr };

        let entry = &table[index];
        frame = match entry.frame() {
            Ok(frame) => frame,
            Err(FrameError::FrameNotPresent) => return None,
            Err(FrameError::HugeFrame) => panic!("huge pages not supported"),
        };
    }

    // 物理アドレスを計算
    Some(frame.start_address() + u64::from(addr.page_offset()))
}

/// フレームをゼロクリアしてページテーブルとして初期化する
fn init_page_table(frame: PhysFrame, physical_memory_offset: VirtAddr) {
    let virt = physical_memory_offset + frame.start_address().as_u64();
    unsafe {
        core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, 4096);
    }
}

/// PageTableEntry が指すテーブルへの参照を返す
unsafe fn table_from_entry(entry: &PageTableEntry, physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    if !entry.flags().contains(PageTableFlags::PRESENT) {
        panic!("table_from_entry: pte does not present");
    }

    let phys = entry.addr();
    let virt = physical_memory_offset + phys.as_u64();
    unsafe { &mut *virt.as_mut_ptr() }
}

/// PhysFrame から PageTable への参照を返す
unsafe fn table_from_frame(frame: PhysFrame, physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let virt = physical_memory_offset + frame.start_address().as_u64();
    unsafe { &mut *virt.as_mut_ptr() }
}

/// 物理アドレス ->  仮想アドレス変換
pub unsafe fn phys_to_virt(phys: PhysAddr, physical_memory_offset: VirtAddr) -> VirtAddr {
    VirtAddr::new(physical_memory_offset.as_u64() + phys.as_u64())
}

/// 仮想アドレス -> 物理アドレスに変換
unsafe fn virt_to_phys(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    translate_addr_inner(addr, physical_memory_offset)
}

/// PageTableEntry の物理アドレスを取得
/// フラグビットを除く
fn pte_phys_addr(entry: &PageTableEntry) -> PhysAddr {
    PhysAddr::new(entry.addr().as_u64())
}

/// xv6 の walkpgdir に相当する 4段ページテーブルウォーカー
/// `va` に対応する PT エントリへの可変参照を返す
/// `alloc == true` の場合、途中のテーブルが存在しなければ新たにフレームを割り当てる
///
/// # Safety
/// - `pml4` は有効な PML4 テーブルへの可変参照でなければならない
/// - `physical_memory_offset` はブートローダから受け取った物理メモリオフセットでなければならない
/// - `alloc == true` の場合、frame_allocator が有効なフレームを返すことを仮定する
pub unsafe fn walk_pagetable<'a, A>(pml4: &'a mut PageTable, va: VirtAddr, alloc: bool, physical_memory_offset: VirtAddr, frame_allocator: &mut A) -> Option<&'a mut PageTableEntry>
where
    A: FrameAllocator<Size4KiB>,
{
    // Level 4 (PML4) to Level 3 (PDPT)
    let pdpt: &mut PageTable = {
        let entry = &mut pml4[pml4_index(va)];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            if !alloc {
                return None;
            }
            let frame = frame_allocator.allocate_frame()?;
            let table_virt = unsafe { phys_to_virt(frame.start_address(), physical_memory_offset) };
            unsafe {
                (table_virt.as_mut_ptr::<PageTable>()).write(PageTable::new());
            }
            entry.set_frame(frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE);
        }
        let phys = pte_phys_addr(entry);
        unsafe { &mut *(phys_to_virt(phys, physical_memory_offset).as_mut_ptr::<PageTable>()) }
    };

    // Level 3 (PDPT) to Level 2 (PD)
    let pd: &mut PageTable = {
        let entry = &mut pdpt[pdpt_index(va)];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            if !alloc {
                return None;
            }
            let frame = frame_allocator.allocate_frame()?;
            let table_virt = unsafe { phys_to_virt(frame.start_address(), physical_memory_offset) };
            unsafe {
                (table_virt.as_mut_ptr::<PageTable>()).write(PageTable::new());
            }
            entry.set_frame(frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE);
        }
        let phys = pte_phys_addr(entry);
        unsafe { &mut *(phys_to_virt(phys, physical_memory_offset).as_mut_ptr::<PageTable>()) }
    };

    // Level 2 (PD) to Level 1 (PT)
    let pt: &mut PageTable = {
        let entry = &mut pd[pd_index(va)];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            if !alloc {
                return None;
            }
            let frame = frame_allocator.allocate_frame()?;
            let table_virt = unsafe { phys_to_virt(frame.start_address(), physical_memory_offset) };
            unsafe {
                (table_virt.as_mut_ptr::<PageTable>()).write(PageTable::new());
            }
            entry.set_frame(frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE);
        }
        let phys = pte_phys_addr(entry);
        unsafe { &mut *(phys_to_virt(phys, physical_memory_offset).as_mut_ptr::<PageTable>()) }
    };

    Some(&mut pt[pt_index(va)])
}
