use crate::println;
use crate::thread;
use crate::exec;

use abi::*;

/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(syscall_num: Syscall, arg1: i64, arg2: i64, arg3: i64, tf: *mut thread::trapframe::TrapFrame) -> SysRet {
    {
        let cpu = crate::cpu::CPU.lock();
        let tid = cpu.current_tid.expect("no current thread");
        drop(cpu);
        let mut table = crate::thread::THREAD_TABLE.lock();
        table[tid].tf = Some(tf);
    }

    match syscall_num {
        abi::SYS_PRINT_NUM => sys_print_num(arg1),
        abi::SYS_PRINT_STR => sys_print_str(arg1 as u64, arg2),
        abi::SYS_FORK => sys_fork(),
        abi::SYS_EXEC => sys_exec(arg1 as u64, arg2 as u64),
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
    crate::println!("[syscall] print_num: {}", n);
    abi::RET_SUCCESS
}

/// 文字列を表示する（ポインタ + 長さ）
fn sys_print_str(ptr: u64, len: i64) -> SysRet {
    // ユーザポインタの検証（今は簡易版）
    if len > 256 {
        return SysRet::MAX;
    }
    let bytes = match super::copy_bytes_from_user(ptr, len as usize) {
        Ok(bytes) => bytes,
        Err(e) => {
            crate::println!("[syscall] print_str copy error: {}", e);
            return SysRet::MAX;
        }
    };
    if let Ok(s) = core::str::from_utf8(&bytes) {
        crate::println!("[syscall] print_str: {}", s);
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
fn sys_exec(path_ptr: u64, argv_ptr: u64) -> SysRet {
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
