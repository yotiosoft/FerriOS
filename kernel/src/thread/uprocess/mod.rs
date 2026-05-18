use alloc::string::ToString;
use spin::Mutex;
use x86_64::{ VirtAddr, registers::control::Cr3, structures::paging::{ FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, PhysFrame, PageTable } };
use lazy_static::lazy_static;
use abi::ProcessID;

use crate::{ memory, exec };

use super::{ THREAD_TABLE, ThreadState };

mod uthread;
pub mod syscalls;

/// ユーザコード
pub const USER_CODE_START: u64 = 0x0000_1000_0000_0000;

/// ユーザスタック
pub const USER_STACK_TOP: u64 = 0x0000_2000_0000_0000;

/// 最大プロセス数
pub const NPROCESS: usize = 16;

/// 1プロセスあたりの最大スレッド数
pub const NTHREAD_PER_PROCESS: usize = 8;

/// Process State
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Unused,
    Alive,
    Zombie,
}

/// Process Control Block (PCB)
#[derive(Debug, Clone, Copy)]
pub struct Process {
    pub pid: ProcessID,                                     // Process ID
    pub ppid: Option<ProcessID>,                            // Parent Process ID
    pub state: ProcessState,                                // Process State
    pub threads: [Option<usize>; NTHREAD_PER_PROCESS],      // Threads the process owns
    pub nthread: usize,                                     // Threads count
    pub page_table: Option<PhysFrame>,                      // Page Table of this process
    pub exit_status: abi::RetValue,                         // Exit return value
}

impl Process {
    pub const fn new() -> Self {
        Process {
            pid: 0,
            ppid: None,
            state: ProcessState::Unused,
            threads: [None; NTHREAD_PER_PROCESS],
            nthread: 0,
            page_table: None,
            exit_status: abi::RET_SUCCESS,
        }
    }

    /// スレッドをプロセスに追加
    pub fn add_thread(&mut self, tid: usize) -> Result<(), &'static str> {
        if self.nthread >= NTHREAD_PER_PROCESS {
            return Err("too many threads in process");
        }
        self.threads[self.nthread] = Some(tid);
        self.nthread += 1;
        Ok(())
    }
}

lazy_static! {
    /// Process Table
    pub static ref PROCESS_TABLE: Mutex<[Option<Process>; NPROCESS]> = Mutex::new([None; NPROCESS]);
}

pub fn create_user_process(code: &[u8], frame_allocator: &mut impl FrameAllocator<Size4KiB>, parent_pagetable: Option<&mut PageTable>) -> Result<(), &'static str> {
    // ユーザページのフラグ
    let user_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    // ユーザページテーブルを作成
    let (mut user_mapper, page_table) = if let Some(parent_pagetable) = parent_pagetable {
        memory::umem::copy_uvm(frame_allocator, parent_pagetable)
    }
    else {
        memory::umem::new_uvm(frame_allocator)
    }?;

    // コードページ用領域を用意
    let code_page = Page::containing_address(VirtAddr::new(USER_CODE_START));
    let code_frame = frame_allocator.allocate_frame().expect("frame alloc failed");

    // コードページにユーザコードをコピー
    let physical_memory = {
        let guard = memory::PHYSICAL_MEMORY_OFFSET.lock();
        guard.expect("physical memory offset not initialized")
    };
    let dst: *mut u8 = (physical_memory + code_frame.start_address().as_u64()).as_mut_ptr();
    unsafe {
        core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
    }
    
    // コードページをユーザページテーブルにマップ
    unsafe {
        user_mapper.map_to(code_page, code_frame, user_flags, frame_allocator)
            .map_err(|_| "code map_to failed")?.flush();
    }

    // ユーザスタック用領域を用意
    let stack_start = USER_STACK_TOP - memory::STACK_SIZE as u64;
    for i in 0..memory::STACK_PAGES as u64 {
        let page = Page::containing_address(VirtAddr::new(stack_start + i * memory::PAGE_SIZE as u64));
        let frame = frame_allocator.allocate_frame().ok_or("frame alloc failed")?;
        unsafe {
            user_mapper.map_to(page, frame, user_flags, frame_allocator).map_err(|_| "stack map_to failed")?.flush();
        }
    }

    // プロセスを作成
    let mut process = alloc_proc()?;

    // ページテーブルを登録
    process.page_table = Some(page_table);

    // プロセスを Process Table に追加
    add_to_process_table(process)?;

    // 全スレッドを Runnable としてマーク
    mark_threads_as_runnable(process)?;

    Ok(())
}

/// 実行する ELF ファイルを指定してユーザプロセスを生成
pub fn create_user_process_from_path(path: &str) -> Result<(), &'static str> {
    let elf = exec::user_programs::lookup(path).ok_or("program not found")?;
    let prepared = exec::prepare_exec_image(elf, &[])?;

    let mut process = alloc_proc()?;
    process.page_table = Some(prepared.page_table);

    {
        let mut thread_table = THREAD_TABLE.lock();
        let tid = process.threads[0].expect("no first thread");
        let thread = &mut thread_table[tid];
        thread.context.rsp3 = prepared.user_sp;
        thread.context.user_rip = prepared.entry;
        thread.context.user_rdi = prepared.argc as u64;
        thread.context.user_rsi = prepared.argv_user_ptr;

        let tf = thread.tf.ok_or("no trapframe")?;
        unsafe {
            (*tf).rdi = prepared.argc as u64;
            (*tf).rsi = prepared.argv_user_ptr;
            (*tf).rcx = prepared.entry;
        }
    }

    add_to_process_table(process)?;
    mark_threads_as_runnable(process)?;

    Ok(())
}

/// 新規プロセス作成
fn alloc_proc() -> Result<Process, &'static str> {
    // Process ID を決定
    let pid = next_pid()?;

    // 1st thread を作成
    let mut first_thread = uthread::create_user_thread()?;
    first_thread.pid = Some(pid);

    // プロセス構造体
    let mut process = Process {
        pid: pid,
        ppid: None,
        state: ProcessState::Alive,
        threads: [None; 8],
        nthread: 0,
        page_table: None,
        exit_status: abi::RET_SUCCESS,
    };

    // 1st thread を追加
    process.add_thread(first_thread.tid)?;

    // Thread Table に追加
    let mut thread_table = THREAD_TABLE.lock();
    thread_table[first_thread.tid] = first_thread;

    Ok(process)
}

/// PID 割り当て
fn next_pid() -> Result<ProcessID, &'static str> {
    let table = PROCESS_TABLE.lock();
    for i in 0..NPROCESS-1 {
        if table[i].is_none() {
            return Ok(i);
        }
    }
    Err("Process table is full")
}

/// プロセスを Process Table に追加
fn add_to_process_table(process: Process) -> Result<(), &'static str> {
    let pid = process.pid;
    if pid >= NPROCESS {
        return Err("Process table is full");
    }

    let mut process_table = PROCESS_TABLE.lock();
    process_table[process.pid] = Some(process);

    Ok(())
}

/// プロセス内の全スレッドを Runnable としてマークする
fn mark_threads_as_runnable(process: Process) -> Result<(), &'static str> {
    let mut thread_table = THREAD_TABLE.lock();

    for thread_element in process.threads {
        if let Some(tid) = thread_element {
            if tid > super::NTHREAD {
                return Err("tid > NTHREAD");
            }
            thread_table[tid].state = ThreadState::Runnable;
        }
    }

    Ok(())
}

/// プロセスを解放する
fn free_process(process: &mut Process) -> Result<(), &'static str> {
    {
        let mut thread_table = THREAD_TABLE.lock();
        for tid in process.threads.into_iter().flatten() {
            if tid >= super::NTHREAD {
                return Err("tid >= NTHREAD");
            }

            let thread = &mut thread_table[tid];
            if thread.pid != Some(process.pid) {
                return Err("thread does not belong to process");
            }

            if thread.kstack != 0 {
                let layout = alloc::alloc::Layout::from_size_align(memory::STACK_SIZE, 16)
                    .map_err(|_| "invalid kernel stack layout")?;
                let kstack_base = (thread.kstack - memory::STACK_SIZE as u64) as *mut u8;
                unsafe {
                    alloc::alloc::dealloc(kstack_base, layout);
                }
            }

            // TrapFrame はカーネルスタック上にあるので、スタック解放後に参照を残さない
            *thread = super::Thread::new();
        }
    }

    if let Some(page_table) = process.page_table {
        // 現在の CR3 active なページでないことを確認
        let (current_page_table, _) = Cr3::read();
        if current_page_table == page_table {
            return Err("cannot free cr3 active page table");
        }

        // free_uvm
        process.page_table = None;
        let mut frame_allocator_guard = memory::FRAME_ALLOCATOR.lock();
        let frame_allocator = frame_allocator_guard
            .as_mut()
            .ok_or("FRAME_ALLOCATOR not initialized")?;
        memory::umem::free_uvm(page_table, frame_allocator)?;
    }

    Ok(())
}
