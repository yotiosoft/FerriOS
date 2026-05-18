use crate::memory;
use crate::cpu;
use crate::scheduler::yield_from_syscall_context;
use crate::thread;
use crate::thread::ThreadState;
use crate::thread::uprocess::PROCESS_TABLE;
use crate::thread::uprocess::ProcessState;
use abi::RetValue;
use x86_64::structures::paging::PageTable;
use abi::{ ProcessID, ThreadID };

static INIT_PID: ProcessID = 0;
static INIT_TID: ThreadID = 0;

pub fn fork() -> Result<ProcessID, &'static str> {
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
            memory::va::phys_to_virt(phys_frame.start_address(), physical_memory_offset)
        };
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }
    };

    // proces state (page table) をコピー
    let (_, page_table) = memory::umem::copy_uvm(frame_allocator, &mut current_process_pml4)?;

    // ページテーブル設定
    process.page_table = Some(page_table);

    // ppid (parent pid) を設定
    {
        let cpu = crate::cpu::CPU.lock();
        process.ppid = cpu.current_pid();
    }

    // 親プロセスの trapframe とユーザ復帰情報をコピー
    let (parent_tf, parent_user_rsp): (thread::trapframe::TrapFrame, u64) = {
        let cpu = crate::cpu::CPU.lock();
        let tid = cpu.current_tid.expect("no current thread");
        let saved_user_rsp = cpu.saved_user_rsp;
        drop(cpu);
        let thread_table = crate::thread::THREAD_TABLE.lock();
        (unsafe { *thread_table[tid].tf.expect("no trapframe") }, saved_user_rsp)
    };
    {
        let mut table = crate::thread::THREAD_TABLE.lock();
        let child_tid = process.threads[0].expect("no child thread");
        let child = &mut table[child_tid];

        // xv6: *np->tf = *proc->tf
        unsafe { *child.tf.expect("no trapframe") = parent_tf; }

        // xv6: np->tf->eax = 0 (子の fork 戻り値を 0 に)
        unsafe { (*child.tf.expect("no trapframe")).rax = 0; }

        // 子は fork から復帰する形で最初にユーザ空間へ入る
        child.context.rsp3 = parent_user_rsp;
        child.context.user_rip = parent_tf.rcx;
        child.context.user_rdi = parent_tf.rdi;
        child.context.user_rsi = parent_tf.rsi;
        
        crate::debug!(
            "[fork] child pid={}, tid={}, user_rip={:#x}, rsp3={:#x}, rax={:#x}",
            process.pid,
            child_tid,
            child.context.user_rip,
            child.context.rsp3,
            unsafe { (*child.tf.expect("no trapframe")).rax },
        );
    }

    // ステータスの設定

    // process_table に追加
    let mut process_table = super::PROCESS_TABLE.lock();
    process_table[process.pid] = Some(process);
    
    // runnable に設定
    super::mark_threads_as_runnable(process)?;

    Ok(process.pid)
}

pub fn getpid() -> Result<ProcessID, &'static str> {
    let cpu = cpu::CPU.lock();
    let pid = cpu.current_pid();

    if let Some(pid) = pid {
        return Ok(pid);
    }
    else {
        return Err("no process")?;
    }
}

pub fn exit(ret_value: abi::RetValue) -> Result<(), &'static str> {
    let pid = cpu::CPU.lock().current_pid().ok_or("no process")?;
    if pid == INIT_PID {
        return Err("init exiting");
    }

    // ToDo: このプロセスが開いている全てのファイルを close する

    // ToDo: このプロセスを wait している親プロセスを wakeup させる

    // ToDo: このプロセスの子プロセスを init thread の子プロセスに変更する

    let mut process = {
        let mut process_table = PROCESS_TABLE.lock();
        let process = process_table[pid].as_mut().ok_or("no process")?;

        // Process Table 上の実体を ZOMBIE 状態に変更する
        process.state = super::ProcessState::Zombie;

        // Exit return value のセット
        process.exit_status = ret_value;

        *process
    };

    // プロセスに属するスレッドを ZOMBIE 状態にする
    {
        let mut thread_table = super::THREAD_TABLE.lock();
        for tid in process.threads {
            if let Some(tid) = tid {
                if let Some(pid) = thread_table[tid].pid {
                    if pid == process.pid {
                        thread_table[tid].state = ThreadState::Zombie;
                    }
                }
            }
        }
    }

    // スケジューラに移行する
    // このシステムコールが return することはない（コンパイルを通すため Ok(()) を返してるけど…）
    yield_from_syscall_context();

    panic!("zonbie exit");

    Ok(())
}

pub fn wait() -> Result<(ProcessID, abi::RetValue), &'static str> {
    let pid = cpu::CPU.lock().current_pid().expect("no pid");

    loop {
        // ZONBIE 状態に子プロセスを探索
        let (mut zombie_child, havekids) = {
            let mut process_table = PROCESS_TABLE.lock();
            let mut zombie_child = None;
            let mut havekids = false;

            for i in 0..super::NPROCESS-1 {
                let is_child = process_table[i]
                    .as_ref()
                    .is_some_and(|child| child.ppid == Some(pid));
                if !is_child {
                    continue;
                }

                havekids = true;
                let is_zombie = process_table[i]
                    .as_ref()
                    .is_some_and(|child| child.state == ProcessState::Zombie);
                if is_zombie {
                    zombie_child = process_table[i].take();
                    break;
                }
            }

            (zombie_child, havekids)
        };

        // ZOMBIE 状態の子プロセスがあった場合
        if let Some(child) = zombie_child.as_mut() {
            // free process
            super::free_process(child)?;

            return Ok((child.pid, child.exit_status));
        }

        if !havekids {
            return Err("no child process");
        }

        yield_from_syscall_context();
    }
}
