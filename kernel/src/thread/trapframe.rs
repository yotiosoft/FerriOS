#[derive(Clone, Copy)]
#[repr(C)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
    pub r11: u64,
    pub rcx: u64,
}

impl TrapFrame {
    pub fn new() -> Self {
        TrapFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbp: 0,
            rbx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rax: 0,
            r11: 0,
            rcx: 0,
        }
    }
}
