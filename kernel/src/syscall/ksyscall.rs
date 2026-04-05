use crate::println;
use crate::thread;
use crate::exec;

/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(syscall_num: u64, arg1: u64, arg2: u64, arg3: u64, tf: *mut thread::trapframe::TrapFrame) -> u64 {
    {
        let cpu = crate::cpu::CPU.lock();
        let tid = cpu.current_tid.expect("no current thread");
        drop(cpu);
        let mut table = crate::thread::THREAD_TABLE.lock();
        table[tid].tf = Some(tf);
    }

    match syscall_num {
        super::SYS_PRINT_NUM => sys_print_num(arg1),
        super::SYS_PRINT_STR => sys_print_str(arg1, arg2),
        super::SYS_FORK => sys_fork(),
        _ => {
            crate::println!("[syscall] unknown syscall: {}", syscall_num);
            u64::MAX  // エラー
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

/// fork
fn sys_fork() -> u64 {
    let ret = thread::uprocess::syscalls::fork();
    if let Err(e) = ret {
        println!("{}", e);
        return 1;
    }
    println!("success");
    return 0;
}

/// exec
fn sys_exec(path_ptr: u64, argv_ptr: u64) -> u64 {
    let path = super::copy_cstr_from_user(path_ptr, super::MAX_ARG_LEN).expect("exec: failed to copy_cstr_from_user");
    let path = core::str::from_utf8(&path).expect("exec: path is not valid UTF-8");
    let argv = super::copy_argv(argv_ptr).expect("exec: failed to copy argv");

    let ret = crate::exec::exec(path, &argv);
    if let Err(e) = ret {
        println!("{}", e);
        return 1;
    }
    0
}
