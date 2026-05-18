use crate::println;
use crate::thread;
use crate::exec;
use crate::interrupts;

use abi::*;

/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(syscall_num: SyscallNum, arg1: i64, arg2: i64, arg3: i64, tf: *mut thread::trapframe::TrapFrame) -> SysRet {
    set_trapframe(tf);
    
    match syscall_num {
        abi::SYS_PRINT_NUM => sys_print_num(arg1),
        abi::SYS_PRINT_STR => sys_print_str(arg1 as u64, arg2),
        abi::SYS_FORK => sys_fork(),
        abi::SYS_EXEC => sys_exec(arg1 as u64, arg2 as u64),
        abi::SYS_GETPID => sys_getpid(),
        abi::SYS_UPTIME => sys_uptime(),
        abi::SYS_EXIT => sys_exit(arg1 as RetValue),
        abi::SYS_WAIT => sys_wait(arg1 as UserAddress),
        _ => {
            crate::println!("[syscall] unknown syscall: {}", syscall_num);
            SysRet::MAX  // エラー
        }
    }
}

/// TrapFrame をセットする
#[unsafe(no_mangle)]
pub extern "C" fn set_trapframe(tf_ptr: *mut thread::trapframe::TrapFrame) {
    let tid = {
        let cpu = crate::cpu::CPU.lock();
        cpu.current_tid
    }.expect("no current thread");
    
    let mut table = crate::thread::THREAD_TABLE.lock();
    table[tid].tf = Some(tf_ptr);
}

/// 数値を表示する
fn sys_print_num(n: i64) -> SysRet {
    crate::println!("{}", n);
    abi::RET_SUCCESS
}

/// 文字列を表示する（ポインタ + 長さ）
fn sys_print_str(ptr: abi::UserAddress, len: i64) -> SysRet {
    // ユーザポインタの検証（今は簡易版）
    if len > 256 {
        return SysRet::MAX;
    }
    let bytes = match super::copy_bytes_from_user(ptr, len as usize) {
        Ok(bytes) => bytes,
        Err(e) => {
            crate::println!("print_str copy error: {}", e);
            return SysRet::MAX;
        }
    };
    if let Ok(s) = core::str::from_utf8(&bytes) {
        crate::println!("{}", s);
        abi::RET_SUCCESS
    } else {
        SysRet::MAX
    }
}

/// fork
fn sys_fork() -> SysRet {
    match thread::uprocess::syscalls::fork() {
        Ok(child_pid) => child_pid as SysRet,
        Err(e) => {
            println!("{}", e);
            abi::RET_ERROR
        }
    }
}

/// exec
fn sys_exec(path_ptr: abi::UserAddress, argv_ptr: abi::UserAddress) -> SysRet {
    let path = super::copy_cstr_from_user(path_ptr, super::MAX_ARG_LEN).expect("exec: failed to copy_cstr_from_user");
    let path = core::str::from_utf8(&path).expect("exec: path is not valid UTF-8");
    let argv = super::copy_argv(argv_ptr).expect("exec: failed to copy argv");

    let ret = crate::exec::exec(path, &argv);
    if let Err(e) = ret {
        println!("{}", e);
        return abi::RET_ERROR;
    }
    0
}

/// getpid
fn sys_getpid() -> SysRet {
    let pid = thread::uprocess::syscalls::getpid();
    if let Ok(pid) = pid {
        return pid as SysRet;
    }
    else {
        return abi::RET_ERROR;
    }
}

/// uptime
fn sys_uptime() -> SysRet {
    let ticks = interrupts::get_ticks();
    if ticks > SysRet::MAX as u64 {
        return abi::RET_ERROR;
    }
    return ticks as SysRet;
}

/// exit
fn sys_exit(ret_value: abi::RetValue) -> SysRet {
    thread::uprocess::syscalls::exit(ret_value);
    panic!("why exit returns..?");
    return -1;
}

/// wait
fn sys_wait(status_ptr: abi::UserAddress) -> SysRet {
    if let Ok((pid, exit_status)) = thread::uprocess::syscalls::wait() {
        if pid > SysRet::MAX as usize {
            return abi::RET_ERROR;
        }

        if status_ptr != abi::NULL_POINTER {
            let bytes = exit_status.to_ne_bytes();
            if super::copy_to_user(status_ptr, &bytes).is_err() {
                return abi::RET_ERROR;
            }
        }

        return pid as SysRet;
    }
    return abi::RET_ERROR;
}
