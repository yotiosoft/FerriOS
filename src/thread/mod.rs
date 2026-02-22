use crate::scheduler;
use scheduler::context::Context;
use crate::cpu;

pub mod kthread;
pub mod uthread;

extern crate alloc;

pub static STACK_SIZE: usize = 4096 * 4;

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

/// 現在実行中のスレッドの tid を取得
pub fn current_tid() -> Option<usize> {
    let cpu = cpu::CPU.lock();
    cpu.current_tid
}
