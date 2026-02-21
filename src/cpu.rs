use crate::scheduler::context;

pub struct Cpu {
    pub id: usize,                      // CPU ID
    pub scheduler: context::Context,    // スケジューラ用コンテキスト
    pub current_pid: Option<usize>,     // 現在実行中のプロセス ID
}

impl Cpu {
    pub fn new(cpu_id: usize) -> Self {
        Cpu {
            id: cpu_id,
            scheduler: context::Context::new(),
            current_pid: None,
        }
    }
}
