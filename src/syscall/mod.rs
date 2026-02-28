use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::structures::gdt::SegmentSelector;
use x86_64::VirtAddr;
use crate::gdt;
use core::arch::naked_asm;

static mut SYSCALL_STACK: [u8; 4096 * 4] = [0; 4096 * 4];
static mut SAVED_USER_RSP: u64 = 0;
static mut KERNEL_SYSCALL_RSP_TOP: u64 = 0;

pub fn init() {
    unsafe {
        Efer::update(|flags| *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS);
    }

    // syscall handler のアドレスを LSTAR に登録
    LStar::write(VirtAddr::new(syscall_entry as u64));

    // CC/SS セグメントを START に設定
    let user_cs_ring0 = SegmentSelector(gdt::GDT.1.user_code_selector.0 & !0b11);
    let user_ss_ring0 = SegmentSelector(gdt::GDT.1.user_data_selector.0 & !0b11);
    Star::write(
        user_cs_ring0,
        user_ss_ring0,
        gdt::GDT.1.kernel_code_selector,
        gdt::GDT.1.kernel_data_selector,
    ).unwrap();

    // カーネル用 syscall スタックをセット
    let stack_top = core::ptr::addr_of!(SYSCALL_STACK) as u64 + (4096 * 4);
    unsafe {
        KERNEL_SYSCALL_RSP_TOP = stack_top;
    }

    // syscall 呼び出し時に IF をクリアさせる
    SFMask::write(x86_64::registers::rflags::RFlags::INTERRUPT_FLAG);
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    // RSP を切り替える
    // ユーザの RSP3 を対比して kstack に切り替え
    // TSS の RSP0 を読み出す
    naked_asm!(
        // カーネル用 syscall スタックに切り替え
        "mov [{user_rsp}], rsp",
        "mov rsp, [{kernel_rsp}]",

        // rsp3 をカーネルの GS に退避
        "mov gs:[0], rsp",
        "mov rsp, gs:[8]",

        // カーネルスタックにレジスタを退避
        "push rcx",
        "push r11",
        "push rax",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // syscall handler を呼ぶ
        "mov rdi, rax",
        "mov rsi, [rsp + 8*8]",
        "mov rdx, [rsp + 7*8]",
        "mov rcx, [rsp + 6*8]",
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

        // rsp3 を復元
        "mov rsp, [{user_rsp}]",
        "swapgs",

        // ユーザモードに戻る
        "sysretq",

        user_rsp    = sym SAVED_USER_RSP,
        kernel_rsp  = sym KERNEL_SYSCALL_RSP_TOP,
        syscall_dispatch = sym syscall_dispatch,
    )
}

/// システムコール番号
pub const SYS_PRINT_NUM: u64 = 0;
pub const SYS_PRINT_STR: u64 = 1;

/// Rustから呼ばれるディスパッチャ
/// 戻り値はRAXに入る
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match nr {
        SYS_PRINT_NUM => sys_print_num(arg1),
        SYS_PRINT_STR => sys_print_str(arg1, arg2),
        _ => {
            crate::println!("[syscall] unknown syscall: {}", nr);
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
