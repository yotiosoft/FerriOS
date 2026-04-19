#![no_std]

use core::arch::asm;
pub use abi::*;

const EXEC_MAX_ARGC: usize = 16;
const EXEC_MAX_ARG_LEN: usize = 256;

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

fn copy_c_string(src: &str, dst: &mut [u8; EXEC_MAX_ARG_LEN + 1]) -> Result<(), ()> {
    let bytes = src.as_bytes();
    if bytes.len() > EXEC_MAX_ARG_LEN {
        return Err(());
    }

    dst[..bytes.len()].copy_from_slice(bytes);
    dst[bytes.len()] = 0;
    Ok(())
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

pub fn exec(path: &str, argv: &[&str]) -> u64 {
    if argv.len() > EXEC_MAX_ARGC {
        return RET_ERROR;
    }

    let mut path_buf = [0u8; EXEC_MAX_ARG_LEN + 1];
    if copy_c_string(path, &mut path_buf).is_err() {
        return RET_ERROR;
    }

    let mut arg_bufs = [[0u8; EXEC_MAX_ARG_LEN + 1]; EXEC_MAX_ARGC];
    let mut argv_ptrs = [0u64; EXEC_MAX_ARGC + 1];

    for (i, arg) in argv.iter().enumerate() {
        if copy_c_string(arg, &mut arg_bufs[i]).is_err() {
            return RET_ERROR;
        }
        argv_ptrs[i] = arg_bufs[i].as_ptr() as u64;
    }

    unsafe {
        syscall(SYS_EXEC, path_buf.as_ptr() as u64, argv_ptrs.as_ptr() as u64, 0)
    }
}

pub fn getpid() -> u64 {
    unsafe {
        syscall(SYS_GETPID, 0, 0, 0)
    }
}
