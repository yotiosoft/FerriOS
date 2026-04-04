use core::ptr;
use alloc::format;
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
        core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, 4096);
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

/// ページテーブルにページをマップする
pub fn map_page(user_mapper: &mut OffsetPageTable<'static>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, page: Page, flags: PageTableFlags) -> Result<(), &'static str> {
    let frame = frame_allocator.allocate_frame().ok_or("map_page: frame alloc failed")?;
    let physical_memory_offset = super::PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");
    let frame_va = unsafe {
        phys_to_virt(frame.start_address(), physical_memory_offset)
    };
    unsafe {
        ptr::write_bytes(frame_va.as_mut_ptr::<u8>(), 0, super::PGSIZE);
        user_mapper.map_to(page, frame, flags, frame_allocator).map_err(|e| format!("map_page: map_to failed. {:?}", e));
    }
    Ok(())
}
