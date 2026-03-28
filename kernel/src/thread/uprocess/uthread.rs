use crate::memory;

use super::{ THREAD_TABLE, USER_STACK_TOP, USER_CODE_START, ThreadState };
use crate::{gdt, thread::Thread};

pub fn create_user_thread() -> Result<Thread, &'static str> {
    // スレッド ID を確保
    let tid = super::super::next_tid().ok_or("Thread table is full")?;

    // スレッドテーブルに追加
    let mut thread = Thread::new();
    thread.tid = tid;
    thread.state = ThreadState::Embryo;

    // カーネルスタックを作成
    memory::setup_kstack(&mut thread);

    // コンテキストを初期化する
    thread.context.rip = ring3_entry_trampoline as u64;
    thread.context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
    thread.context.cs = gdt::GDT.1.user_code_selector.0 as u64;
    thread.context.ss = gdt::GDT.1.user_data_selector.0 as u64;
    thread.context.rsp3 = USER_STACK_TOP;

    Ok(thread)
}

unsafe extern "C" fn ring3_entry_trampoline() -> ! {
    let (cs, ss, rsp3, rip) = {
        let table = THREAD_TABLE.lock();
        let ctx =&table[super::super::current_tid().expect("No running thread")].context;
        (ctx.cs, ctx.ss, ctx.rsp3, USER_CODE_START)
    };

    let rip = USER_CODE_START;
    unsafe {
        core::arch::asm!(
            "mov ds, ax",
            "mov es, ax",
            "push rax",
            "push {rsp3}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            // clear the registers (the values ​​we need are already on the stack)
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rdi, rdi",
            "xor rsi, rsi",
            "xor r8,  r8",
            "xor r9,  r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",
            "iretq",            // switch: cs, ss, rsp, rflags
            inout("ax") ss => _,
            cs = in(reg) cs,
            rsp3 = in(reg) rsp3,
            rflags = in(reg) 0x202u64,
            rip = in(reg) rip,
        );
    }

    loop {}
}
