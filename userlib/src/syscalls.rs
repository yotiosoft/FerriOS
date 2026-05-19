use super::*;

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

pub fn uptime() -> SysRet {
    unsafe {
        syscall(SYS_UPTIME, 0, 0, 0)
    }
}

pub fn exit(ret_value: abi::RetValue) -> ! {
    unsafe {
        syscall(SYS_EXIT, ret_value, 0, 0);
    }
    panic!("exit returns!");
}

pub fn wait(status_ptr: Option<&mut abi::RetValue>) -> SysRet {
    let status_ptr = match status_ptr {
        Some(p) => p as *mut abi::RetValue as i64,
        None => 0,
    };

    unsafe {
        syscall(SYS_WAIT, status_ptr, 0, 0)
    }
}
