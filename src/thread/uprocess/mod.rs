use spin::Mutex;
use x86_64::{ VirtAddr, structures::paging::{ FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, PhysFrame } };
use lazy_static::lazy_static;

use crate::memory;

use super::{ STACK_SIZE, THREAD_TABLE, ThreadState };

mod uthread;

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

pub fn create_user_process(code: &[u8], mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<(), &'static str> {
    // ユーザページのフラグ
    let user_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    // ユーザページテーブルを作成
    let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET.lock().expect("physical memory offset not initialized");
    let (mut user_mapper, page_table) = unsafe {
        memory::create_user_page_table(frame_allocator, physical_memory_offset)
    }.ok_or("failed to allocate page table frame")?;

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

    // カーネルスタックを作成
    let kstack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };
    let kstack_top = kstack as u64 + STACK_SIZE as u64;

    // init thread を作成
    let thread = uthread::create_user_thread(kstack_top);

    // Thread Table に追加
    let tid = thread.tid;
    let mut thread_table = THREAD_TABLE.lock();
    thread_table[tid] = thread;

    // Process ID を決定
    let pid = next_pid()?;

    // Thread 構造体に pid を設定
    thread_table[tid].pid = Some(pid);

    // Process 構造体を作成
    let mut process = Process {
        pid: pid,
        threads: [None; 8],
        nthread: 1,
        page_table: Some(page_table),
    };
    process.add_thread(tid)?;

    // Process Table に追加
    let mut process_table = PROCESS_TABLE.lock();
    process_table[pid] = Some(process);

    Ok(())
}

fn next_pid() -> Result<usize, &'static str> {
    let table = PROCESS_TABLE.lock();
    for i in 0..NPROCESS-1 {
        if table[i].is_none() {
            return Ok(i);
        }
    }
    Err("Process table is full")
}
