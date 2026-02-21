use crate::scheduler;
use scheduler::context::Context;

extern crate alloc;

static STACK_SIZE: usize = 4096 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Unused,
    Embryo,
    Sleeping,
    Runnable,
    Running,
    Zombie,
}

/// Process Control Block
#[derive(Debug, Clone, Copy)]
pub struct Thread {
    pub tid: usize,             // Thread ID
    pub state: ThreadState,     // スレッドの状態
    pub context: Context,       // スレッドのコンテキスト
    pub kstack: u64,            // このスレッド用のカーネルスタック
}

impl Thread {
    pub fn new() -> Self {
        Thread {
            tid: 0,
            state: ThreadState::Unused,
            context: Context::new(),
            kstack: 0,
        }
    }
}

pub const NTHREAD: usize = 64;
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref THREAD_TABLE: Mutex<[Thread; NTHREAD]> = {
        Mutex::new([Thread::new(); NTHREAD])
    };
}

/// カーネルスレッド作成
pub fn create_kernel_thread(entry: fn() -> !) {
    // スレッド ID を確保
    let tid = next_tid().expect("Thread table is full");

    // スタックを作成
    let stack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };
    let stack_top = stack as u64 + STACK_SIZE as u64;

    let mut table = THREAD_TABLE.lock();
    table[tid].tid = tid;
    table[tid].state = ThreadState::Runnable;
    table[tid].kstack = stack_top;

    // コンテキストを初期化する
    table[tid].context.rsp = stack_top;
    table[tid].context.rip = entry as u64;
    table[tid].context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
}

/// スレッド ID 決定
pub fn next_tid() -> Option<usize> {
    let table = THREAD_TABLE.lock();
    for i in 0..NTHREAD-1 {
        if table[i].state == ThreadState::Unused {
            return Some(i);
        }
    }
    None
}
