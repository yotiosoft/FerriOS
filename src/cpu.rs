use crate::scheduler::context;

pub struct Cpu {
    pub id: usize,                      // CPU ID
    pub scheduler: context::Context,    // スケジューラ用コンテキスト
    pub current_tid: Option<usize>,     // 現在実行中のスレッド ID
}

impl Cpu {
    pub fn new(cpu_id: usize) -> Self {
        Cpu {
            id: cpu_id,
            scheduler: context::Context::new(),
            current_tid: None,
        }
    }
}
