use crate::memory;
use crate::cpu;
use crate::thread;
use x86_64::structures::paging::PageTable;

pub fn fork() -> Result<(), &'static str> {
    // プロセス割り当て
    let mut process = super::alloc_proc()?;

    // frame allocator を取得
    let mut guard = memory::FRAME_ALLOCATOR.lock();
    let frame_allocator = guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    // 現在のプロセスの PML4 page table を取得
    let mut current_process_pml4: &mut PageTable = {
        let cpu = cpu::CPU.lock();
        let phys_frame = cpu.current_process().expect("process not found").page_table.expect("no page table");

        let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");

        // PhysFrame → 仮想アドレス → &mut PageTable
        let virt = unsafe {
            memory::phys_to_virt(phys_frame.start_address(), physical_memory_offset)
        };
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }
    };

    // proces state (page table) をコピー
    let (_, page_table) = memory::copy_uvm(frame_allocator, &mut current_process_pml4)?;

    // ページテーブル設定
    process.page_table = Some(page_table);

    // ステータスの設定

    // process_table に追加
    let mut process_table = super::PROCESS_TABLE.lock();
    process_table[process.pid] = Some(process);
    
    // runnable に設定
    super::mark_threads_as_runnable(process)?;

    Ok(())
}
