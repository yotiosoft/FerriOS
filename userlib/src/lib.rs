#![no_std]

use core::arch::asm;
use abi::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "mov rax, rdi",
            "mov rdi, rsi",
            "mov rsi, rdx",
            "mov rdx, rcx",
            "syscall",
            inlateout("rdi") num => _,
            inlateout("rsi") arg1 => _,
            inlateout("rdx") arg2 => _,
            inlateout("rcx") arg3 => _,
            lateout("rax") ret,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn print_num(num: u64) -> u64 {
    unsafe {
        syscall(SYS_PRINT_NUM, num, 0, 0)
    }
}

pub fn print_str(s: &str) -> u64 {
    unsafe {
        syscall(SYS_PRINT_STR, s.as_ptr() as u64, s.len() as u64, 0)
    }
}

pub fn fork() -> u64 {
    unsafe {
        syscall(SYS_FORK, 0, 0, 0)
    }
}

pub fn exec(path_ptr: &str, argv_ptr: u64) -> u64 {
    unsafe {
        syscall(SYS_EXEC, path_ptr.as_ptr() as u64, argv_ptr, 0)
    }
}

pub fn getpid() -> u64 {
    unsafe {
        syscall(SYS_GETPID, 0, 0, 0)
    }
}
