use crate::process::context;

pub struct Cpu {
    pub id: usize,                      // CPU ID
    pub scheduler: context::Context,    // スケジューラ用コンテキスト
}

impl Cpu {
    pub fn new(cpu_id: usize) -> Self {
        Cpu {
            id: cpu_id,
            scheduler: context::Context::new(),
        }
    }
}
