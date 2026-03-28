use alloc::string::ToString;
use spin::Mutex;
use x86_64::{ VirtAddr, structures::paging::{ FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, PhysFrame, PageTable } };
use lazy_static::lazy_static;

use crate::{memory, thread};

use super::{ STACK_SIZE, THREAD_TABLE, ThreadState };

mod uthread;
pub mod syscalls;

/// ユーザコード
pub const USER_CODE_START: u64 = 0x0000_1000_0000_0000;

/// ユーザスタック
pub const USER_STACK_TOP: u64 = 0x0000_2000_0000_0000;
pub const USER_STACK_PAGES: u64 = 4;

/// 最大プロセス数
pub const NPROCESS: usize = 16;

/// 1プロセスあたりの最大スレッド数
pub const NTHREAD_PER_PROCESS: usize = 8;

/// Process Control Block (PCB)
#[derive(Debug, Clone, Copy)]
pub struct Process {
    pub pid: usize,
    pub threads: [Option<usize>; NTHREAD_PER_PROCESS],
    pub nthread: usize,
    pub page_table: Option<PhysFrame>,
}

impl Process {
    pub const fn new() -> Self {
        Process {
            pid: 0,
            threads: [None; NTHREAD_PER_PROCESS],
            nthread: 0,
            page_table: None,
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
        memory::copy_uvm(frame_allocator, parent_pagetable)
    }
    else {
        memory::new_uvm(frame_allocator)
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
    let stack_start = USER_STACK_TOP - USER_STACK_PAGES * 4096;
    for i in 0..USER_STACK_PAGES {
        let page = Page::containing_address(VirtAddr::new(stack_start + i * 4096));
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
        threads: [None; 8],
        nthread: 1,
        page_table: None,
    };

    // 1st thread を追加
    process.add_thread(first_thread.tid)?;

    // Thread Table に追加
    let mut thread_table = THREAD_TABLE.lock();
    thread_table[first_thread.tid] = first_thread;

    Ok(process)
}

/// PID 割り当て
fn next_pid() -> Result<usize, &'static str> {
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
