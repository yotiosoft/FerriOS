use super::{ Thread, ThreadState, THREAD_TABLE, NTHREAD, CPU, SCHEDULER_STARTED };
use super::context::{ Context, switch_context };

pub struct RoundRobin;

impl super::Scheduler for RoundRobin {
    /// スケジューラ
    fn scheduler(&self) -> ! {
        unsafe {
            if SCHEDULER_STARTED {
                panic!("Scheduler already started");
            }
            SCHEDULER_STARTED = true;
        }

        loop {
            let mut table = THREAD_TABLE.lock();
            let mut cpu = CPU.lock();
            
            // 次に実行するスレッドの決定
            let next_pid = {
                find_next_runnable_thread(&table, cpu.current_pid)
            };

            match next_pid {
                None => {
                    x86_64::instructions::interrupts::enable_and_hlt();
                    drop(cpu);
                    drop(table);
                    continue;
                }
                Some(next_pid) => {
                    let (old_context, new_context) = {
                        // プロセス状態を更新
                        table[next_pid].state = ThreadState::Running;
                        if let Some(current_pid) = cpu.current_pid {
                            if table[current_pid].state == ThreadState::Running {
                                table[current_pid].state = ThreadState::Runnable;
                            }
                        }
                        
                        // CPU で実行中のプロセス ID を更新
                        cpu.current_pid = Some(next_pid);
                        
                        let old_context = &mut cpu.scheduler as *mut Context;
                        let new_context = &table[next_pid].context as *const Context;

                        drop(cpu);
                        drop(table);

                        (old_context, new_context)
                    };

                    unsafe {
                        x86_64::instructions::interrupts::enable();
                        //crate::println!("switch");
                        switch_context(old_context, new_context);
                    }
                }
            }
        }
    }

    /// スレッドからスケジューラに戻る
    fn on_yield(&self) {
        x86_64::instructions::interrupts::disable();

        let mut table = THREAD_TABLE.lock();
        let cpu = CPU.lock();

        let current_pid = cpu.current_pid;
        if current_pid.is_none() {
            x86_64::instructions::interrupts::enable();
            return;
        }
        let current_pid = current_pid.unwrap();
        if table[current_pid].state != ThreadState::Running {
            panic!("CPU has current_pid but the thread is not Running");
        }

        let (old_context, new_context) = {
            // Runnable に変更
            table[current_pid].state = ThreadState::Runnable;

            // スケジューラへコンテキストスイッチ
            let old_context = &mut table[current_pid].context as *mut Context;
            let new_context = &cpu.scheduler as *const Context;

            drop(cpu);
            drop(table);

            (old_context, new_context)
        };
        unsafe {
            x86_64::instructions::interrupts::enable();
            switch_context(old_context, new_context);
        }
    }
}

fn find_next_runnable_thread(table: &[Thread; NTHREAD], current_pid: Option<usize>) -> Option<usize> {
    let current_pid = current_pid.unwrap_or(0);
    for i in 1..NTHREAD+1 {
        let pid = (current_pid + i) % NTHREAD;
        if table[pid].state == ThreadState::Runnable {
            return Some(pid);
        }
    }
    None
}
