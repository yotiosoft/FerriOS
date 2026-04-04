use super::{ STACK_SIZE, THREAD_TABLE, ThreadState };
use crate::memory;

pub const NTHREAD: usize = 64;

/// カーネルスレッド作成
pub fn create_kernel_thread(entry: fn() -> !) {
    // スレッド ID を確保
    let tid = super::next_tid().expect("Thread table is full");

    let mut table = THREAD_TABLE.lock();
    table[tid].tid = tid;
    table[tid].state = ThreadState::Runnable;

    // カーネルスタックを用意する
    memory::kmem::setup_kstack(&mut table[tid]);

    // コンテキストを初期化する
    table[tid].context.rip = super::kthread_entry as u64;
    table[tid].entry = Some(entry);
    table[tid].context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
}
