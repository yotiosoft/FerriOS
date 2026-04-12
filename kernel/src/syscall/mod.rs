use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::VirtAddr;
use core::arch::naked_asm;
use core::mem::offset_of;
use alloc::vec::Vec;

use crate::gdt;
use crate::cpu::Cpu;

mod ksyscall;

const OFFSET_SAVED_USER_RSP: usize = offset_of!(Cpu, saved_user_rsp);
const OFFSET_KERNEL_SYSCALL_RSP: usize = offset_of!(Cpu, kernel_syscall_rsp);

pub fn init() -> Result<(), &'static str> {
    unsafe {
        Efer::update(|flags| *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS);
    }

    // syscall handler のアドレスを LSTAR に登録
    LStar::write(VirtAddr::new(syscall_entry as u64));

    // CC/SS セグメントを STAR に設定
    Star::write(
        gdt::GDT.1.user_code_selector,
        gdt::GDT.1.user_data_selector,
        gdt::GDT.1.kernel_code_selector,
        gdt::GDT.1.kernel_data_selector,
    )?;

    // syscall 呼び出し時に IF をクリアさせる
    SFMask::write(x86_64::registers::rflags::RFlags::INTERRUPT_FLAG);

    Ok(())
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // カーネル用 GS に切り替え
        "swapgs",

        // ユーザ RSP を退避し、カーネルスタックに切り替え
        "mov gs:[{saved_user_rsp}], rsp",
        "mov rsp, gs:[{kernel_syscall_rsp}]",

        // push する前に syscall番号を別レジスタに退避
        "mov r10, rax",

        // レジスタを退避
        "push rcx",   // sysretq 用 RIP
        "push r11",   // sysretq 用 RFLAGS
        "push rax",   // syscall 番号
        "push rdi",
        "push rsi",
        "push rdx",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // syscall_dispatch(number=rax, arg0=rdi, arg1=rsi, arg2=rdx)
        // 引数は rdi, rsi, rdx に入っている
        "mov r8,  rsp",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, r10",
        // rsi, rdx はユーザが設定した値がそのまま残っている
        "call {syscall_dispatch}",
        // syscall_dispatch の戻り値を退避する
        "mov r10, rax",

        // レジスタを復元
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        // 保存していた syscall 番号は破棄する
        "add rsp, 8",
        "pop r11",
        "pop rcx",
        // ユーザへ返す戻り値を rax に戻す
        "mov rax, r10",

        // ユーザ RSP を復元
        "mov rsp, gs:[{saved_user_rsp}]",

        // ユーザ用 GS に戻す
        "swapgs",

        // ユーザモードに戻る
        "sysretq",

        saved_user_rsp     = const OFFSET_SAVED_USER_RSP,
        kernel_syscall_rsp = const OFFSET_KERNEL_SYSCALL_RSP,
        syscall_dispatch   = sym ksyscall::syscall_dispatch,
    )
}

/// 引数
const MAX_ARGC: usize = 16;
const MAX_ARG_LEN: usize = 256;

fn copy_argv(argv_ptr: u64) -> Result<Vec<Vec<u8>>, &'static str> {
    let mut argv = Vec::new();
    if argv_ptr == 0 {
        return Ok(argv);
    }

    for i in 0..MAX_ARGC {
        let user_arg_ptr = unsafe { *((argv_ptr as *const u64).add(i)) };
        if user_arg_ptr == 0 {
            return Ok(argv);
        }
        argv.push(copy_cstr_from_user(user_arg_ptr, MAX_ARG_LEN)?);
    }

    Err("exec: too many arguments")
}

fn copy_cstr_from_user(ptr: u64, max_len: usize) -> Result<Vec<u8>, &'static str> {
    if ptr == 0 {
        return Err("exec: null argument pointer");
    }

    let mut bytes = Vec::new();
    for i in 0..max_len {
        let byte = unsafe { *((ptr as *const u8).add(i)) };
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
    }

    Err("exec: argument is too long")
}
