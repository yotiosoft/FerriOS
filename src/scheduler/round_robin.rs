use crate::memory;
use crate::thread;

use super::{ Thread, ThreadState, THREAD_TABLE, NTHREAD, cpu::CPU, SCHEDULER_STARTED };
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
            let next_tid = {
                find_next_runnable_thread(&table, cpu.current_tid)
            };

            match next_tid {
                None => {
                    x86_64::instructions::interrupts::enable_and_hlt();
                    drop(cpu);
                    drop(table);
                    continue;
                }
                Some(next_tid) => {
                    let (old_context, new_context, next_pid) = {
                        // スレッド状態を更新
                        table[next_tid].state = ThreadState::Running;
                        if let Some(current_tid) = cpu.current_tid {
                            if table[current_tid].state == ThreadState::Running {
                                table[current_tid].state = ThreadState::Runnable;
                            }
                        }
                        
                        // CPU で実行中のスレッド ID を更新
                        cpu.current_tid = Some(next_tid);
                        
                        let old_context = &mut cpu.scheduler as *mut Context;
                        let new_context = &table[next_tid].context as *const Context;

                        // CR3 page table switch
                        if table[next_tid].pid.is_some() {
                            // ユーザスレッドの場合：プロセスのユーザページテーブルに切り替え
                            unsafe {
                                table[next_tid].switch_to_user_page_table();
                            }
                        }
                        else {
                            // カーネルスレッドの場合：カーネルページテーブルに切り替え
                            unsafe {
                                memory::switch_to_kernel_page_table();
                            }
                        }

                        let next_pid = table[next_tid].pid;

                        drop(cpu);
                        drop(table);

                        (old_context, new_context, next_pid)
                    };

                    match next_pid {
                        Some(pid) => {
                            let process_table = thread::uprocess::PROCESS_TABLE.lock();
                            if let Some(process) = &process_table[pid] {
                                if let Some(frame) = process.page_table {
                                    unsafe {
                                        x86_64::registers::control::Cr3::write(frame, x86_64::registers::control::Cr3Flags::empty());
                                    }
                                }
                            }
                        }
                        None => {
                            unsafe {
                                memory::switch_to_kernel_page_table();
                            }
                        }
                    }

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

        // カーネルページテーブルに切り替え
        unsafe {
            memory::switch_to_kernel_page_table();
        }

        let mut table = THREAD_TABLE.lock();
        let cpu = CPU.lock();

        let current_tid = cpu.current_tid;
        if current_tid.is_none() {
            x86_64::instructions::interrupts::enable();
            return;
        }
        let current_tid = current_tid.unwrap();
        if table[current_tid].state != ThreadState::Running {
            panic!("CPU has current_tid but the thread is not Running");
        }

        let (old_context, new_context) = {
            // Runnable に変更
            table[current_tid].state = ThreadState::Runnable;

            // スケジューラへコンテキストスイッチ
            let old_context = &mut table[current_tid].context as *mut Context;
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

fn find_next_runnable_thread(table: &[Thread; NTHREAD], current_tid: Option<usize>) -> Option<usize> {
    let current_tid = current_tid.unwrap_or(0);
    for i in 1..NTHREAD+1 {
        let tid = (current_tid + i) % NTHREAD;
        if table[tid].state == ThreadState::Runnable {
            return Some(tid);
        }
    }
    None
}
