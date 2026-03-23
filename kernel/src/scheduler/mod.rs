use crate::thread::{ Thread, ThreadState, THREAD_TABLE, NTHREAD };
use crate::cpu;
use conquer_once::spin::OnceCell;
use alloc::boxed::Box;

pub mod context;
pub mod round_robin;

pub static SCHEDULER: OnceCell<Box<dyn Scheduler + Send + Sync>> = OnceCell::uninit();
pub static mut SCHEDULER_STARTED: bool = false;

pub fn init(scheduler: Box<dyn Scheduler + Send + Sync>) {
    SCHEDULER.init_once(|| scheduler);
}

pub trait Scheduler: Send + Sync {
    fn scheduler(&self) -> !;
    fn on_yield(&self);
}

fn get_scheduler() -> &'static dyn Scheduler {
    SCHEDULER.get()
        .expect("Scheduler not initialized")
        .as_ref()
}

pub fn scheduler() -> ! {
    get_scheduler().scheduler();
}

pub fn yield_from_context() {
    get_scheduler().on_yield();
}
