use super::{ next_pid, STACK_SIZE, PROCESS_TABLE, ProcessState };

/// カーネルスレッド作成
pub fn create_kernel_thread(entry: fn() -> !) {
    // プロセス ID を確保
    let pid = next_pid().expect("Process table is full");

    // スタックを作成
    let stack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };
    let stack_top = stack as u64 + STACK_SIZE as u64;

    let mut table = PROCESS_TABLE.lock();
    table[pid].pid = pid;
    table[pid].state = ProcessState::Runnable;
    table[pid].kstack = stack_top;

    // コンテキストを初期化する
    table[pid].context.rsp = stack_top;
    table[pid].context.rip = entry as u64;
    table[pid].context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
}
