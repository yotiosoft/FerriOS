use crate::scheduler;
use scheduler::context::Context;
use crate::cpu;

pub mod kthread;
pub mod uprocess;

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
    pub pid: Option<usize>,     // Process ID (ユーザプロセスの場合)
    pub state: ThreadState,     // スレッドの状態
    pub context: Context,       // スレッドのコンテキスト
    pub kstack: u64,            // このスレッド用のカーネルスタック
    pub entry: Option<fn() -> !>,       // 実行する関数
}

impl Thread {
    pub fn new() -> Self {
        Thread {
            tid: 0,
            pid: None,
            state: ThreadState::Unused,
            context: Context::new(),
            kstack: 0,
            entry: None,
        }
    }

    pub unsafe fn switch_to_user_page_table(&self) {
        if let Some(pid) = self.pid {
            let process_table = uprocess::PROCESS_TABLE.lock();
            let process = &process_table[pid].expect("this process does not have page table yet");
            let page_table = process.page_table.expect("this process is not in the process_table");

            unsafe {
                x86_64::registers::control::Cr3::write(page_table, x86_64::registers::control::Cr3Flags::empty());
            }
        }
        else {
            panic!("this process does not have pid");
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

/// スケジューラからきりかわた直後に一度だけ実行される関数
/// 割り込みを有効化
extern "C" fn kthread_entry() -> ! {
    x86_64::instructions::interrupts::enable();
    
    // 実際のスレッド関数を呼び出す
    let entry = {
        let table = THREAD_TABLE.lock();
        let cpu = cpu::CPU.lock();
        let tid = cpu.current_tid.unwrap();
        table[tid].entry.expect("entry not set")
    };
    entry();
}
