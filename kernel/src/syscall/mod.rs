use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::VirtAddr;
use core::arch::naked_asm;
use core::mem::offset_of;

use crate::gdt;
use crate::cpu::Cpu;

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
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, r10",
        // rsi, rdx はユーザが設定した値がそのまま残っている
        "call {syscall_dispatch}",

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
        "pop rax",
        "pop r11",
        "pop rcx",

        // ユーザ RSP を復元
        "mov rsp, gs:[{saved_user_rsp}]",

        // ユーザ用 GS に戻す
        "swapgs",

        // ユーザモードに戻る
        "sysretq",

        saved_user_rsp     = const OFFSET_SAVED_USER_RSP,
        kernel_syscall_rsp = const OFFSET_KERNEL_SYSCALL_RSP,
        syscall_dispatch   = sym syscall_dispatch,
    )
}

/// システムコール番号
pub const SYS_PRINT_NUM: u64 = 0;
pub const SYS_PRINT_STR: u64 = 1;

/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(syscall_num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match syscall_num {
        SYS_PRINT_NUM => sys_print_num(arg1),
        SYS_PRINT_STR => sys_print_str(arg1, arg2),
        _ => {
            crate::println!("[syscall] unknown syscall: {}", syscall_num);
            u64::MAX  // エラー
        }
    }
}

/// 数値を表示する
fn sys_print_num(n: u64) -> u64 {
    crate::println!("[syscall] print_num: {}", n);
    0
}

/// 文字列を表示する（ポインタ + 長さ）
fn sys_print_str(ptr: u64, len: u64) -> u64 {
    // ユーザポインタの検証（今は簡易版）
    if len > 256 {
        return u64::MAX;
    }
    let slice = unsafe {
        core::slice::from_raw_parts(ptr as *const u8, len as usize)
    };
    if let Ok(s) = core::str::from_utf8(slice) {
        crate::println!("[syscall] print_str: {}", s);
        0
    } else {
        u64::MAX
    }
}
