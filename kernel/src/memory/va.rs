use core::ptr;
use x86_64::structures::paging::{Mapper, OffsetPageTable};

use crate::memory;

use super::{ FrameAllocator, Size4KiB, VirtAddr, PhysFrame, PageTable, PhysAddr, Page, PageTableEntry, PageTableFlags };

fn pml4_index(va: VirtAddr) -> usize { (va.as_u64() as usize >> 39) & 0x1FF }
fn pdpt_index(va: VirtAddr) -> usize { (va.as_u64() as usize >> 30) & 0x1FF }
fn pd_index  (va: VirtAddr) -> usize { (va.as_u64() as usize >> 21) & 0x1FF }
fn pt_index  (va: VirtAddr) -> usize { (va.as_u64() as usize >> 12) & 0x1FF }

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
pub fn init_page_table(frame: PhysFrame, physical_memory_offset: VirtAddr) {
    let virt = physical_memory_offset + frame.start_address().as_u64();
    unsafe {
        core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, super::PAGE_SIZE);
    }
}

/// PageTableEntry が指すテーブルへの参照を返す
pub unsafe fn table_from_entry(entry: &PageTableEntry, physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    if !entry.flags().contains(PageTableFlags::PRESENT) {
        panic!("table_from_entry: pte does not present");
    }

    let phys = entry.addr();
    let virt = physical_memory_offset + phys.as_u64();
    unsafe { &mut *virt.as_mut_ptr() }
}

/// PhysFrame から PageTable への参照を返す
pub unsafe fn table_from_frame(frame: PhysFrame, physical_memory_offset: VirtAddr) -> &'static mut PageTable {
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

/// 4段ページテーブルウォーカー
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

/// ページテーブルにページをマップする
pub fn map_page(user_mapper: &mut OffsetPageTable<'static>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, page: Page, flags: PageTableFlags) -> Result<(), &'static str> {
    let frame = frame_allocator.allocate_frame().ok_or("map_page: frame alloc failed")?;
    let physical_memory_offset = super::PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");
    let frame_va = unsafe {
        phys_to_virt(frame.start_address(), physical_memory_offset)
    };
    unsafe {
        ptr::write_bytes(frame_va.as_mut_ptr::<u8>(), 0, super::PAGE_SIZE);
        user_mapper
            .map_to(page, frame, flags, frame_allocator)
            .map_err(|_| "map_page: map_to failed")?
            .flush();
    }
    Ok(())
}

/// 連続するページをマップする
pub fn map_pages(user_mapper: &mut OffsetPageTable<'static>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, start_page: Page, num_pages: u64, flags: PageTableFlags) -> Result<(), &'static str> {
    // start_page から num_pages 分を順番に map する
    for i in 0..num_pages {
        let offset = i
            .checked_mul(super::PAGE_SIZE as u64)
            .ok_or("map_pages: page offset overflow")?;
        let va_u64 = start_page
            .start_address()
            .as_u64()
            .checked_add(offset)
            .ok_or("map_pages: virtual address overflow")?;
        let va = VirtAddr::new(va_u64);
        let page = Page::containing_address(va);
        map_page(user_mapper, frame_allocator, page, flags)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHYSICAL_MEMORY_OFFSET: u64 = 0xFFFF_A000_0000_0000;
    const TEST_FRAME_COUNT: usize = 8;

    #[derive(Clone, Copy)]
    #[repr(align(4096))]
    struct AlignedFrame([u8; super::super::PAGE_SIZE]);

    static mut TEST_FRAMES: [AlignedFrame; TEST_FRAME_COUNT] =
        [AlignedFrame([0xcc; super::super::PAGE_SIZE]); TEST_FRAME_COUNT];

    struct TestFrameAllocator {
        next: usize,
        frames: [PhysFrame; TEST_FRAME_COUNT],
    }

    unsafe impl FrameAllocator<Size4KiB> for TestFrameAllocator {
        fn allocate_frame(&mut self) -> Option<PhysFrame> {
            let frame = self.frames.get(self.next).copied();
            self.next += usize::from(frame.is_some());
            frame
        }
    }

    fn physical_memory_offset() -> VirtAddr {
        VirtAddr::new(PHYSICAL_MEMORY_OFFSET)
    }

    fn test_frame(index: usize) -> PhysFrame {
        let frame_ptr = unsafe { core::ptr::addr_of!(TEST_FRAMES[index]) };
        let frame_va = VirtAddr::new(frame_ptr as u64);
        let frame_pa = translate_addr_inner(frame_va, physical_memory_offset())
            .expect("test frame virtual address should be mapped");

        PhysFrame::containing_address(frame_pa)
    }

    fn reset_test_frames() -> TestFrameAllocator {
        unsafe {
            for frame in core::ptr::addr_of_mut!(TEST_FRAMES).as_mut().unwrap() {
                frame.0.fill(0xcc);
            }
        }

        TestFrameAllocator {
            next: 0,
            frames: [
                test_frame(0),
                test_frame(1),
                test_frame(2),
                test_frame(3),
                test_frame(4),
                test_frame(5),
                test_frame(6),
                test_frame(7),
            ],
        }
    }

    fn fresh_page_table(frame_allocator: &mut TestFrameAllocator) -> &'static mut PageTable {
        let frame = frame_allocator
            .allocate_frame()
            .expect("test pml4 frame should be available");
        init_page_table(frame, physical_memory_offset());

        unsafe { table_from_frame(frame, physical_memory_offset()) }
    }

    #[test_case]
    fn page_table_indexes_extract_expected_bits() {
        let va = VirtAddr::new(
            (0x12u64 << 39) | (0x34u64 << 30) | (0x56u64 << 21) | (0x78u64 << 12),
        );

        assert_eq!(pml4_index(va), 0x12);
        assert_eq!(pdpt_index(va), 0x34);
        assert_eq!(pd_index(va), 0x56);
        assert_eq!(pt_index(va), 0x78);
    }

    #[test_case]
    fn phys_to_virt_adds_physical_memory_offset() {
        let phys = PhysAddr::new(0x1234_5000);
        let virt = unsafe { phys_to_virt(phys, physical_memory_offset()) };

        assert_eq!(virt.as_u64(), PHYSICAL_MEMORY_OFFSET + phys.as_u64());
    }

    #[test_case]
    fn init_page_table_zeroes_frame() {
        let frame = test_frame(0);
        init_page_table(frame, physical_memory_offset());
        let table = unsafe { table_from_frame(frame, physical_memory_offset()) };

        assert!(table.iter().all(|entry| entry.is_unused()));
    }

    #[test_case]
    fn walk_pagetable_without_alloc_returns_none_for_missing_mapping() {
        let mut frame_allocator = reset_test_frames();
        let pml4 = fresh_page_table(&mut frame_allocator);
        let va = VirtAddr::new(0x0000_1234_5678);

        let entry = unsafe {
            walk_pagetable(pml4, va, false, physical_memory_offset(), &mut frame_allocator)
        };

        assert!(entry.is_none());
        assert_eq!(frame_allocator.next, 1);
    }

    #[test_case]
    fn walk_pagetable_with_alloc_creates_intermediate_tables() {
        let mut frame_allocator = reset_test_frames();
        let pml4 = fresh_page_table(&mut frame_allocator);
        let va = VirtAddr::new(0x0000_1234_5678);

        let entry = unsafe {
            walk_pagetable(pml4, va, true, physical_memory_offset(), &mut frame_allocator)
        }
        .expect("walk_pagetable should return leaf entry");

        assert!(entry.is_unused());
        assert_eq!(frame_allocator.next, 4);

        let expected_flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        let pml4_entry = &pml4[pml4_index(va)];
        assert!(pml4_entry.flags().contains(expected_flags));

        let pdpt = unsafe { table_from_entry(pml4_entry, physical_memory_offset()) };
        let pdpt_entry = &pdpt[pdpt_index(va)];
        assert!(pdpt_entry.flags().contains(expected_flags));

        let pd = unsafe { table_from_entry(pdpt_entry, physical_memory_offset()) };
        let pd_entry = &pd[pd_index(va)];
        assert!(pd_entry.flags().contains(expected_flags));
    }

    #[test_case]
    fn walk_pagetable_reuses_existing_intermediate_tables() {
        let mut frame_allocator = reset_test_frames();
        let pml4 = fresh_page_table(&mut frame_allocator);
        let va = VirtAddr::new(0x0000_1234_5678);

        unsafe {
            walk_pagetable(pml4, va, true, physical_memory_offset(), &mut frame_allocator)
                .expect("first walk should allocate intermediate tables");
        }
        let used_after_first_walk = frame_allocator.next;

        unsafe {
            walk_pagetable(pml4, va, true, physical_memory_offset(), &mut frame_allocator)
                .expect("second walk should reuse intermediate tables");
        }

        assert_eq!(used_after_first_walk, 4);
        assert_eq!(frame_allocator.next, used_after_first_walk);
    }
}
