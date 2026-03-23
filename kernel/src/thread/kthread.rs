use super::{ STACK_SIZE, THREAD_TABLE, ThreadState };

pub const NTHREAD: usize = 64;

/// カーネルスレッド作成
pub fn create_kernel_thread(entry: fn() -> !) {
    // スレッド ID を確保
    let tid = super::next_tid().expect("Thread table is full");

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
    table[tid].context.rip = super::kthread_entry as u64;
    table[tid].entry = Some(entry);
    table[tid].context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
}
