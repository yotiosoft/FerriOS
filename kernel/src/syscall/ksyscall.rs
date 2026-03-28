/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(syscall_num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match syscall_num {
        super::SYS_PRINT_NUM => sys_print_num(arg1),
        super::SYS_PRINT_STR => sys_print_str(arg1, arg2),
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

/// fork
fn sys_fork() {
    
}
