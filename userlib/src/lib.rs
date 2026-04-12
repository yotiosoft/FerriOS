#![no_std]

use core::arch::asm;

pub const SYS_PRINT_NUM: u64 = 0;
pub const SYS_PRINT_STR: u64 = 1;
pub const SYS_FORK: u64 = 2;
pub const SYS_EXEC: u64 = 3;

struct SyscallArgs {
    arg1: Option<u64>,
    arg2: Option<u64>,
    arg3: Option<u64>,
}
impl Default for SyscallArgs {
    fn default() -> Self {
        SyscallArgs {
            arg1: None,
            arg2: None,
            arg3: None,
        }
    }
}

unsafe fn syscall(num: u64, args: SyscallArgs) -> u64 {
    let ret: u64;
    let arg1 = args.arg1.unwrap_or_default();
    let arg2 = args.arg2.unwrap_or_default();
    let arg3 = args.arg3.unwrap_or_default();

    unsafe {
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    ret
}

pub fn print_num(num: u64) -> u64 {
    let args = SyscallArgs {
        arg1: Some(num),
        ..Default::default()
    };
    unsafe {
        syscall(SYS_PRINT_NUM, args)
    }
}
