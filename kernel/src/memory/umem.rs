use super::{ FrameAllocator, FrameDeallocator, Size4KiB,PhysFrame, PageTable, OffsetPageTable, PHYSICAL_MEMORY_OFFSET, PAGETABLE_USER_SPACE_START, PAGETABLE_USER_SPACE_END, PageTableFlags, va };
use super::kmem;
use super::thread;
use x86_64::structures::paging::page_table::FrameError;

/// ユーザプロセスのページテーブルに切り替え
pub unsafe fn switch_to_user_page_table(thread: &thread::Thread) {
    if let Some(pid) = thread.pid {
        let process_table = thread::uprocess::PROCESS_TABLE.lock();
        let process = &process_table[pid].expect("process_table does not have the process");
        let page_table = process.page_table.expect("this process does not have a page-table");

        unsafe {
            x86_64::registers::control::Cr3::write(page_table, x86_64::registers::control::Cr3Flags::empty());
        }
    }
    else {
        panic!("this process does not have pid");
    }
}

pub fn new_uvm(frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<(OffsetPageTable<'static>, PhysFrame), &'static str> {
    // physical_memory_offset
    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");

    // 新しい level-4 フレームを allocate
    let (new_frame, new_table_ptr) = unsafe {
        kmem::setup_kvm(frame_allocator, physical_memory_offset)
    }?;

    let new_table = unsafe {
        &mut *new_table_ptr
    };

    // ユーザ空間のエントリのみクリア
    let user_code_l4_index = (crate::thread::uprocess::USER_CODE_START >> 39) as usize & 0x1FF;   // 32
    let user_stack_l4_index = (crate::thread::uprocess::USER_STACK_TOP >> 39) as usize & 0x1FF;   // 64
    new_table[user_code_l4_index].set_unused();
    new_table[user_stack_l4_index].set_unused();

    let new_page_table = unsafe {
        OffsetPageTable::new(&mut *new_table_ptr, physical_memory_offset)
    };

    Ok((new_page_table, new_frame))
}

/// 親プロセスのユーザ空間 [0]..[255] を子プロセスにコピー
pub fn copy_uvm(frame_allocator: &mut impl FrameAllocator<Size4KiB>, parent_pml4: &mut PageTable) -> Result<(OffsetPageTable<'static>, PhysFrame), &'static str> {
    // physical_memory_offset
    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");

    // 子の PML4 を作成
    let (child_offset_table, child_pml4_frame) = new_uvm(frame_allocator)?;

    // 子の PML4 への生ポインタを取得
    let child_pml4_virt = physical_memory_offset + child_pml4_frame.start_address().as_u64();
    let child_pml4: &mut PageTable = unsafe { &mut *child_pml4_virt.as_mut_ptr() };

    // ユーザ空間 PML4 エントリ (index 0..255) を走査
    for pml4_idx in PAGETABLE_USER_SPACE_START..PAGETABLE_USER_SPACE_END { // Iterate 0 to 255 (exclusive of 256)
        if !parent_pml4[pml4_idx].flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        // 子の PDPT を新規割り当て
        let child_pdpt_frame = frame_allocator.allocate_frame().ok_or("copy_uvm: failed to allocate PDPT frame")?;
        va::init_page_table(child_pdpt_frame, physical_memory_offset);

        // 子の PML4 エントリに書き込む
        let parent_pdpt_flags = parent_pml4[pml4_idx].flags();
        child_pml4[pml4_idx].set_frame(child_pdpt_frame, parent_pdpt_flags);

        // 親の PDPT を取得
        let parent_pdpt = unsafe {
            va::table_from_entry(&parent_pml4[pml4_idx], physical_memory_offset)
        };
        let child_pdpt = unsafe {
            va::table_from_frame(child_pdpt_frame, physical_memory_offset)
        };

        // PDPT エントリを走査
        for pdpt_idx in 0..512usize {
            if !parent_pdpt[pdpt_idx].flags().contains(PageTableFlags::PRESENT) {
                continue;
            }

            // 子の PD を新規割り当て
            let child_pd_frame = frame_allocator.allocate_frame().ok_or("copy_uvm: failed to allocate PD frame")?;
            va::init_page_table(child_pd_frame, physical_memory_offset);

            let parent_pd_flags = parent_pdpt[pdpt_idx].flags();
            child_pdpt[pdpt_idx].set_frame(child_pd_frame, parent_pd_flags);

            let parent_pd = unsafe {
                va::table_from_entry(&parent_pdpt[pdpt_idx], physical_memory_offset)
            };
            let child_pd = unsafe {
                va::table_from_frame(child_pd_frame, physical_memory_offset)
            };

            // PD エントリを走査
            for pd_idx in 0..512usize {
                if !parent_pd[pd_idx].flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }

                // 子の PT を新規割り当て
                let child_pt_frame = frame_allocator.allocate_frame().ok_or("copy_uvm: failed to allocate PT frame")?;
                va::init_page_table(child_pt_frame, physical_memory_offset);

                let parent_pt_flags = parent_pd[pd_idx].flags();
                child_pd[pd_idx].set_frame(child_pt_frame, parent_pt_flags);

                let parent_pt = unsafe {
                    va::table_from_entry(&parent_pd[pd_idx], physical_memory_offset)
                };
                let child_pt = unsafe {
                    va::table_from_frame(child_pt_frame, physical_memory_offset)
                };

                // PT エントリを走査
                for pt_idx in 0..512usize {
                    let parent_pte = &parent_pt[pt_idx];
                    if !parent_pte.flags().contains(PageTableFlags::PRESENT) {
                        continue;
                    }

                    // 新しい物理フレームを確保
                    let new_frame = frame_allocator
                        .allocate_frame()
                        .ok_or("copy_uvm: failed to allocate data frame")?;

                    let src_virt = physical_memory_offset + parent_pte.addr().as_u64();
                    let dst_virt = physical_memory_offset + new_frame.start_address().as_u64();

                    // ページをコピー
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src_virt.as_ptr::<u8>(),
                            dst_virt.as_mut_ptr::<u8>(),
                            super::PAGE_SIZE,
                        );
                    }

                    // 子の PT エントリに新フレームを書き込む
                    child_pt[pt_idx].set_frame(new_frame, parent_pte.flags());
                }
            }
        }
    }

    Ok((child_offset_table, child_pml4_frame))
}

/// alloc uvm
/// プロセスのユーザ空間を oldsz から newsz まで拡張し、成功時は newsz を返す
pub fn alloc_uvm(pml4: &mut PageTable, oldsz: usize, newsz: usize, frame_allocator: &mut (impl FrameAllocator<Size4KiB> + FrameDeallocator<Size4KiB>)) -> Result<usize, &'static str> {
    let user_space_top = PAGETABLE_USER_SPACE_END.checked_shl(super::PTE_BASE_ADDRESS).ok_or("alloc_uvm: user space limit overflow")?;

    if newsz >= user_space_top {
        return Err("alloc_uvm: newsz is outside user space");
    }
    if newsz < oldsz {
        return Ok(oldsz);
    }

    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");
    let mut addr = page_round_up(oldsz)?;

    while addr < newsz {
        let frame = match frame_allocator.allocate_frame() {
            Some(frame) => frame,
            None => {
                rollback_alloc_uvm(pml4, page_round_up(oldsz)?, addr, frame_allocator)?;
                return Err("alloc_uvm: frame alloc failed");
            }
        };

        let frame_va = physical_memory_offset + frame.start_address().as_u64();
        unsafe {
            core::ptr::write_bytes(frame_va.as_mut_ptr::<u8>(), 0, super::PAGE_SIZE);
        }

        let va = x86_64::VirtAddr::new(addr as u64);
        let pte = unsafe {
            va::walk_pagetable(pml4, va, true, physical_memory_offset, frame_allocator)
        };
        let pte = match pte {
            Some(pte) => pte,
            None => {
                unsafe {
                    frame_allocator.deallocate_frame(frame);
                }
                rollback_alloc_uvm(pml4, page_round_up(oldsz)?, addr, frame_allocator)?;
                return Err("alloc_uvm: page table alloc failed");
            }
        };
        if pte.flags().contains(PageTableFlags::PRESENT) {
            unsafe {
                frame_allocator.deallocate_frame(frame);
            }
            rollback_alloc_uvm(pml4, page_round_up(oldsz)?, addr, frame_allocator)?;
            return Err("alloc_uvm: page is already mapped");
        }

        pte.set_frame(frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE);
        addr = addr.checked_add(super::PAGE_SIZE).ok_or("alloc_uvm: address overflow")?;
    }

    Ok(newsz)
}

/// dealloc uvm
/// プロセスのユーザ空間を oldsz から newsz まで縮小し、成功時は newsz を返す
pub fn dealloc_uvm(pml4: &mut PageTable, oldsz: usize, newsz: usize, frame_deallocator: &mut (impl FrameAllocator<Size4KiB> + FrameDeallocator<Size4KiB>)) -> Result<usize, &'static str> {
    if newsz >= oldsz {
        return Ok(oldsz);
    }

    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");
    let mut addr = page_round_up(newsz)?;

    while addr < oldsz {
        let va = x86_64::VirtAddr::new(addr as u64);
        if let Some(pte) = unsafe { va::walk_pagetable(pml4, va, false, physical_memory_offset, frame_deallocator) } {
            if pte.flags().contains(PageTableFlags::PRESENT) {
                let frame = frame_from_entry(pte)?;
                unsafe {
                    frame_deallocator.deallocate_frame(frame);
                }
                pte.set_unused();
            }
        }
        addr = addr.checked_add(super::PAGE_SIZE).ok_or("dealloc_uvm: address overflow")?;
    }

    Ok(newsz)
}

fn page_round_up(value: usize) -> Result<usize, &'static str> {
    value.checked_add(super::PAGE_SIZE - 1).map(|value| value & !(super::PAGE_SIZE - 1)).ok_or("alloc_uvm: size overflow")
}

fn rollback_alloc_uvm(pml4: &mut PageTable, start: usize, end: usize, frame_deallocator: &mut (impl FrameAllocator<Size4KiB> + FrameDeallocator<Size4KiB>)) -> Result<(), &'static str> {
    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET
        .lock()
        .expect("physical memory offset not initialized");
    let mut addr = start;

    while addr < end {
        let va = x86_64::VirtAddr::new(addr as u64);
        if let Some(pte) = unsafe {
            va::walk_pagetable(pml4, va, false, physical_memory_offset, frame_deallocator)
        } {
            if pte.flags().contains(PageTableFlags::PRESENT) {
                let frame = frame_from_entry(pte)?;
                unsafe {
                    frame_deallocator.deallocate_frame(frame);
                }
                pte.set_unused();
            }
        }
        addr = addr
            .checked_add(super::PAGE_SIZE)
            .ok_or("alloc_uvm: rollback address overflow")?;
    }

    Ok(())
}

/// ユーザ空間のページテーブル階層と、それが参照する leaf frame を解放する
///
/// # Safety contract
/// 呼び出し元は `pml4_frame` が現在使用中の CR3 ではなく、このページテーブル階層が
/// どこからも参照されていないことが保証されている必要あり
pub fn free_uvm(pml4_frame: PhysFrame, frame_deallocator: &mut impl FrameDeallocator<Size4KiB>) -> Result<(), &'static str> {
    let physical_memory_offset = PHYSICAL_MEMORY_OFFSET
        .lock()
        .expect("physical memory offset not initialized");
    let pml4 = unsafe {
        va::table_from_frame(pml4_frame, physical_memory_offset)
    };

    for pml4_idx in PAGETABLE_USER_SPACE_START..PAGETABLE_USER_SPACE_END {
        if !pml4[pml4_idx].flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        let pdpt_frame = frame_from_entry(&pml4[pml4_idx])?;
        let pdpt = unsafe {
            va::table_from_frame(pdpt_frame, physical_memory_offset)
        };

        for pdpt_idx in 0..512usize {
            if !pdpt[pdpt_idx].flags().contains(PageTableFlags::PRESENT) {
                continue;
            }

            let pd_frame = frame_from_entry(&pdpt[pdpt_idx])?;
            let pd = unsafe {
                va::table_from_frame(pd_frame, physical_memory_offset)
            };

            for pd_idx in 0..512usize {
                if !pd[pd_idx].flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }

                let pt_frame = frame_from_entry(&pd[pd_idx])?;
                let pt = unsafe {
                    va::table_from_frame(pt_frame, physical_memory_offset)
                };

                for pt_idx in 0..512usize {
                    if !pt[pt_idx].flags().contains(PageTableFlags::PRESENT) {
                        continue;
                    }

                    let leaf_frame = frame_from_entry(&pt[pt_idx])?;
                    unsafe {
                        frame_deallocator.deallocate_frame(leaf_frame);
                    }
                    pt[pt_idx].set_unused();
                }

                unsafe {
                    frame_deallocator.deallocate_frame(pt_frame);
                }
                pd[pd_idx].set_unused();
            }

            unsafe {
                frame_deallocator.deallocate_frame(pd_frame);
            }
            pdpt[pdpt_idx].set_unused();
        }

        unsafe {
            frame_deallocator.deallocate_frame(pdpt_frame);
        }
        pml4[pml4_idx].set_unused();
    }

    unsafe {
        frame_deallocator.deallocate_frame(pml4_frame);
    }

    Ok(())
}

fn frame_from_entry(entry: &x86_64::structures::paging::page_table::PageTableEntry) -> Result<PhysFrame, &'static str> {
    match entry.frame() {
        Ok(frame) => Ok(frame),
        Err(FrameError::HugeFrame) => Err("huge pages not supported"),
        Err(FrameError::FrameNotPresent) => Err("page table entry not present"),
    }
}

/// 対象プロセスのヒープサイズを拡大縮小する
pub fn grow_process_heap(n: isize, process: &mut thread::uprocess::Process) -> Result<(), &'static str> {
    let old_size = process.heap_size;
    
    let page_table = get_process_page_table(*process)?;

    let mut guard = super::FRAME_ALLOCATOR.lock();
    let frame_allocator = guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    if n > 0 {
        match alloc_uvm(page_table, old_size, old_size + n as usize, frame_allocator) {
            Ok(new_size) => {
                process.heap_size = new_size;
            },
            Err(e) => {
                return Err(e);
            }
        }
    }
    else if n < 0 {
        match dealloc_uvm(page_table, old_size, old_size + n as usize, frame_allocator) {
            Ok(new_size) => {
                process.heap_size = new_size;
            },
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// 対象プロセスの PageTable を取得する
pub fn get_process_page_table(process: thread::uprocess::Process) -> Result<&'static mut PageTable, &'static str> {
    let phys_frame = process.page_table.expect("no page table");

    let physical_memory_offset = super::PHYSICAL_MEMORY_OFFSET
        .lock()
        .expect("physical memory offset not initialized");

    // PhysFrame → 仮想アドレス → &mut PageTable
    let virt =
        unsafe { super::va::phys_to_virt(phys_frame.start_address(), physical_memory_offset) };
    unsafe { Ok(&mut *virt.as_mut_ptr::<PageTable>()) }
}
