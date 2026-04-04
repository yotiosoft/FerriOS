use super::{ KERNEL_PAGE_TABLE_FRAME, FrameAllocator, Size4KiB, VirtAddr, PhysFrame, PageTable, PAGETABLE_KERNEL_SPACE_START, PAGETABLE_KERNEL_SPACE_END };
use x86_64::registers::control::Cr3Flags;

use crate::thread;

/// カーネルページテーブルに切り替え
pub unsafe fn switch_to_kernel_page_table() {
    let kernel_frame = KERNEL_PAGE_TABLE_FRAME.lock();
    if let Some(frame) = *kernel_frame {
        unsafe {
            x86_64::registers::control::Cr3::write(frame, Cr3Flags::empty());
        }
    }
}

/// カーネルスタックを用意する
pub fn setup_kstack(thread: &mut thread::Thread) {
    // カーネルスタックを作成
    let kstack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(thread::STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };
    let kstack_top = kstack as u64 + thread::STACK_SIZE as u64;

    // カーネルスタックの先頭に TrapFrame を確保
    let tf_ptr = (kstack_top - core::mem::size_of::<thread::trapframe::TrapFrame>() as u64) as *mut thread::trapframe::TrapFrame;
    unsafe {
        tf_ptr.write(thread::trapframe::TrapFrame::new());
    }

    thread.kstack = kstack_top;
    thread.context.rsp = kstack_top;
    thread.tf = Some(tf_ptr);
}

/// カーネル空間を map する
pub unsafe fn setup_kvm(frame_allocator: &mut impl FrameAllocator<Size4KiB>, physical_memory_offset: VirtAddr) -> Result<(PhysFrame, *mut PageTable), &'static str> {
    // 新しい level-4 フレームを allocate
    let new_frame = frame_allocator.allocate_frame().ok_or("allocating frame failed")?;

    // 新しいページテーブルを初期化
    let new_table_va = physical_memory_offset + new_frame.start_address().as_u64();
    let new_table_ptr: *mut PageTable = new_table_va.as_mut_ptr();
    unsafe {
        new_table_ptr.write(PageTable::new());
    }

    // カーネル用領域 をコピー
    let (current_frame, _) = x86_64::registers::control::Cr3::read();
    let current_va = physical_memory_offset + current_frame.start_address().as_u64();
    let current_table_ptr: *const PageTable = current_va.as_ptr();
    let current_table = unsafe {
        &*current_table_ptr
    };
    let new_table = unsafe {
        &mut *new_table_ptr
    };

    for i in PAGETABLE_KERNEL_SPACE_START..PAGETABLE_KERNEL_SPACE_END {
        new_table[i] = current_table[i].clone();
    }

    Ok((new_frame, new_table_ptr))
}
