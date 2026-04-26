use crate::{gdt, memory, thread};

use super::{ Thread, ThreadState, THREAD_TABLE, NTHREAD, cpu::CPU, SCHEDULER_STARTED };
use super::context::{ Context, switch_context };

pub struct RoundRobin;

impl super::Scheduler for RoundRobin {
    /// スケジューラ
    fn scheduler(&self) -> ! {
        x86_64::instructions::interrupts::disable();

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
                    drop(cpu);
                    drop(table);
                    x86_64::instructions::interrupts::enable_and_hlt();
                    continue;
                }
                Some(next_tid) => {
                    let (old_context, new_context) = {
                        // スレッド状態を更新
                        table[next_tid].state = ThreadState::Running;
                        if let Some(current_tid) = cpu.current_tid {
                            if table[current_tid].state == ThreadState::Running {
                                table[current_tid].state = ThreadState::Runnable;
                            }
                        }
                        
                        // CPU で実行中のスレッド ID を更新
                        cpu.current_tid = Some(next_tid);
                        
                        // CR3 page table switch
                        if table[next_tid].pid.is_some() {
                            // ユーザスレッドの場合：プロセスのユーザページテーブルに切り替え
                            unsafe {
                                memory::umem::switch_to_user_page_table(&table[next_tid]);
                            }
                        }
                        else {
                            // カーネルスレッドの場合：カーネルページテーブルに切り替え
                            unsafe {
                                memory::kmem::switch_to_kernel_page_table();
                            }
                        }
                        
                        let old_context = &mut cpu.scheduler as *mut Context;
                        let new_context = &table[next_tid].context as *const Context;

                        // CPU の syscall_rsp をスレッドの kstack に変更
                        cpu.kernel_syscall_rsp = table[next_tid].kstack;
                        // ユーザモードからの割り込み/例外は TSS.rsp0 を使う。
                        // TrapFrame 用に確保した領域を踏まないよう、その直前を使う。
                        gdt::set_privilege_stack_0(
                            table[next_tid].kstack
                                - core::mem::size_of::<thread::trapframe::TrapFrame>() as u64
                        );

                        drop(cpu);
                        drop(table);

                        (old_context, new_context)
                    };

                    unsafe {
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
            memory::kmem::switch_to_kernel_page_table();
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
            switch_context(old_context, new_context);
        }

        x86_64::instructions::interrupts::enable();
    }
}

fn find_next_runnable_thread(table: &[Thread; NTHREAD], current_tid: Option<usize>) -> Option<usize> {
    let current_tid = current_tid.unwrap_or(NTHREAD - 1);
    for i in 1..NTHREAD+1 {
        let tid = (current_tid + i) % NTHREAD;
        if table[tid].state == ThreadState::Runnable {
            return Some(tid);
        }
    }
    None
}
