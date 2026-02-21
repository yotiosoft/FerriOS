/// コンテキスト構造体
/// コンテキストスイッチ時の保存/復元に利用
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
    pub rflags: u64,
}

impl Context {
    pub fn new() -> Self {
        Context {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
            rflags: 0x0,
        }
    }
}

use core::arch::global_asm;

unsafe extern "C" {
    pub fn switch_context(old: *mut Context, new: *const Context);
}

// コンテキストスイッチ
global_asm!(
r#"
.globl switch_context
switch_context:
    # 現在のコンテキストを保存
    mov [rdi + 0], r15
    mov [rdi + 8], r14
    mov [rdi + 16], r13
    mov [rdi + 24], r12
    mov [rdi + 32], rbx
    mov [rdi + 40], rbp
    
    # RSP を保存
    lea rax, [rsp + 8]
    mov [rdi + 48], rax
    
    # RIP を保存
    mov rax, [rsp]
    mov [rdi + 56], rax
    
    # RFLAGS を保存
    pushfq
    pop rax
    mov [rdi + 64], rax
    
    # 新しいコンテキストを復元
    mov r15, [rsi + 0]
    mov r14, [rsi + 8]
    mov r13, [rsi + 16]
    mov r12, [rsi + 24]
    mov rbx, [rsi + 32]
    mov rbp, [rsi + 40]
    mov rsp, [rsi + 48]
    
    # RFLAGS を復元
    mov rax, [rsi + 64]
    push rax
    popfq
    
    # 新しいプロセスへ jump
    push qword ptr [rsi + 56]
    ret
"#
);
