use crate::scheduler::context;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CPU: spin::Mutex<Cpu> = {
        let mut cpu = Cpu::new(0);
        cpu.init_syscall_stack();
        spin::Mutex::new(cpu)
    };
}

pub struct Cpu {
    pub id: usize,                      // CPU ID
    pub scheduler: context::Context,    // スケジューラ用コンテキスト
    pub current_tid: Option<usize>,     // 現在実行中のスレッド ID
    pub saved_user_rsp: u64,            // システムコール呼び出し時のユーザ側の RSP
    pub kernel_syscall_rsp: u64,        // システムコール呼び出し時の RSP
    pub syscall_stack: [u8; 4096 * 4],  // システムコール用スタック
}

impl Cpu {
    pub fn new(cpu_id: usize) -> Self {
        Cpu {
            id: cpu_id,
            scheduler: context::Context::new(),
            current_tid: None,
            saved_user_rsp: 0,
            kernel_syscall_rsp: 0,
            syscall_stack: [0; 4096 * 4],
        }
    }

    pub fn init_syscall_stack(&mut self) {
        let stack_top = self.syscall_stack.as_ptr() as u64
            + self.syscall_stack.len() as u64;
        self.kernel_syscall_rsp = stack_top;
    }
}

pub fn init() {
    use x86_64::registers::model_specific::KernelGsBase;
    use x86_64::VirtAddr;

    let cpu_ptr = &*CPU.lock() as *const Cpu as u64;
    KernelGsBase::write(VirtAddr::new(cpu_ptr));
}
