use super::{ FrameAllocator, Size4KiB,PhysFrame, PageTable, OffsetPageTable, PHYSICAL_MEMORY_OFFSET, PAGETABLE_USER_SPACE_START, PAGETABLE_USER_SPACE_END, PageTableFlags, init_page_table, table_from_entry, table_from_frame };
use super::kmem;
use super::thread;

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
        init_page_table(child_pdpt_frame, physical_memory_offset);

        // 子の PML4 エントリに書き込む
        let parent_pdpt_flags = parent_pml4[pml4_idx].flags();
        child_pml4[pml4_idx].set_frame(child_pdpt_frame, parent_pdpt_flags);

        // 親の PDPT を取得
        let parent_pdpt = unsafe {
            table_from_entry(&parent_pml4[pml4_idx], physical_memory_offset)
        };
        let child_pdpt = unsafe {
            table_from_frame(child_pdpt_frame, physical_memory_offset)
        };

        // PDPT エントリを走査
        for pdpt_idx in 0..512usize {
            if !parent_pdpt[pdpt_idx].flags().contains(PageTableFlags::PRESENT) {
                continue;
            }

            // 子の PD を新規割り当て
            let child_pd_frame = frame_allocator.allocate_frame().ok_or("copy_uvm: failed to allocate PD frame")?;
            init_page_table(child_pd_frame, physical_memory_offset);

            let parent_pd_flags = parent_pdpt[pdpt_idx].flags();
            child_pdpt[pdpt_idx].set_frame(child_pd_frame, parent_pd_flags);

            let parent_pd = unsafe {
                table_from_entry(&parent_pdpt[pdpt_idx], physical_memory_offset)
            };
            let child_pd = unsafe {
                table_from_frame(child_pd_frame, physical_memory_offset)
            };

            // PD エントリを走査
            for pd_idx in 0..512usize {
                if !parent_pd[pd_idx].flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }

                // 子の PT を新規割り当て
                let child_pt_frame = frame_allocator.allocate_frame().ok_or("copy_uvm: failed to allocate PT frame")?;
                init_page_table(child_pt_frame, physical_memory_offset);

                let parent_pt_flags = parent_pd[pd_idx].flags();
                child_pd[pd_idx].set_frame(child_pt_frame, parent_pt_flags);

                let parent_pt = unsafe {
                    table_from_entry(&parent_pd[pd_idx], physical_memory_offset)
                };
                let child_pt = unsafe {
                    table_from_frame(child_pt_frame, physical_memory_offset)
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
                            4096,
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
