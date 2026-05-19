#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};
pub use abi::*;

const EXEC_MAX_ARGC: usize = 16;
const EXEC_MAX_ARG_LEN: usize = 256;
const PRINT_FMT_BUF_LEN: usize = 256;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall(num: SyscallNum, arg1: i64, arg2: i64, arg3: i64) -> SysRet {
    let ret: SysRet;
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

pub fn print_num(num: i64) -> SysRet {
    unsafe {
        syscall(SYS_PRINT_NUM, num, 0, 0)
    }
}

pub fn print_str(s: &str) -> SysRet {
    unsafe {
        syscall(SYS_PRINT_STR, s.as_ptr() as i64, s.len() as i64, 0)
    }
}

pub fn print_fmt(args: fmt::Arguments<'_>) -> SysRet {
    let mut buf = StackBuf::new();
    if buf.write_fmt(args).is_err() {
        return RET_ERROR;
    }
    print_str(buf.as_str())
}

pub fn fork() -> SysRet {
    unsafe {
        syscall(SYS_FORK, 0, 0, 0)
    }
}

pub fn exec(path: &str, argv: &[&str]) -> SysRet {
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
        syscall(SYS_EXEC, path_buf.as_ptr() as i64, argv_ptrs.as_ptr() as i64, 0)
    }
}

pub fn getpid() -> SysRet {
    unsafe {
        syscall(SYS_GETPID, 0, 0, 0)
    }
}

struct StackBuf {
    bytes: [u8; PRINT_FMT_BUF_LEN],
    len: usize,
}

impl StackBuf {
    const fn new() -> Self {
        Self {
            bytes: [0; PRINT_FMT_BUF_LEN],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for StackBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let new_len = self.len.checked_add(bytes.len()).ok_or(fmt::Error)?;
        if new_len > self.bytes.len() {
            return Err(fmt::Error);
        }

        self.bytes[self.len..new_len].copy_from_slice(bytes);
        self.len = new_len;
        Ok(())
    }
}

#[macro_export]
macro_rules! print_fmt {
    ($($arg:tt)*) => {{
        $crate::print_fmt(core::format_args!($($arg)*))
    }};
}
