use spin::Mutex;
use lazy_static::lazy_static;

pub mod context;
pub mod scheduler;
pub mod kthread;
pub mod uprocess;

extern crate alloc;

use context::Context;

static STACK_SIZE: usize = 4096 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Unused,
    Embryo,
    Sleeping,
    Runnable,
    Running,
    Zombie,
}

/// Process Control Block
#[derive(Debug, Clone, Copy)]
pub struct Process {
    pub pid: usize,             // Process ID
    pub state: ProcessState,    // プロセスの状態
    pub context: Context,       // プロセスのコンテキスト
    pub kstack: u64,            // このプロセス用のカーネルスタック
}

impl Process {
    pub fn new() -> Self {
        Process {
            pid: 0,
            state: ProcessState::Unused,
            context: Context::new(),
            kstack: 0,
        }
    }
}

// プロセス数
pub const NPROC: usize = 64;

lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<[Process; NPROC]> = {
        Mutex::new([Process::new(); NPROC])
    };
}

/// プロセス ID 決定
pub fn next_pid() -> Option<usize> {
    let table = PROCESS_TABLE.lock();
    for i in 0..NPROC-1 {
        if table[i].state == ProcessState::Unused {
            return Some(i);
        }
    }
    None
}
