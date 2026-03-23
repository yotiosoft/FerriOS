use crate::scheduler::context;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CPU: spin::Mutex<Cpu> = spin::Mutex::new(Cpu::new(0));
}

pub struct Cpu {
    pub id: usize,                      // CPU ID
    pub scheduler: context::Context,    // スケジューラ用コンテキスト
    pub current_tid: Option<usize>,     // 現在実行中のスレッド ID
    pub saved_user_rsp: u64,            // システムコール呼び出し前のユーザ側の RSP
    pub kernel_syscall_rsp: u64,        // システムコール呼び出し時のカーネルの RSP
}

impl Cpu {
    pub fn new(cpu_id: usize) -> Self {
        Cpu {
            id: cpu_id,
            scheduler: context::Context::new(),
            current_tid: None,
            saved_user_rsp: 0,
            kernel_syscall_rsp: 0,
        }
    }
}

pub fn init() {
    use x86_64::registers::model_specific::KernelGsBase;
    use x86_64::VirtAddr;

    let cpu_ptr = &*CPU.lock() as *const Cpu as u64;
    crate::println!("cpu_ptr: {:#x}", cpu_ptr);
    crate::println!("kernel_syscall_rsp offset: {}", core::mem::offset_of!(Cpu, kernel_syscall_rsp));
    KernelGsBase::write(VirtAddr::new(cpu_ptr));
    crate::println!("KernelGsBase written: {:#x}", KernelGsBase::read().as_u64());
}
