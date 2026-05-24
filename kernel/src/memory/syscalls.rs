use super::*;
use crate::{cpu, memory::umem::grow_process_heap};

pub fn sbrk(n: isize) -> Result<abi::UserAddress, &'static str> {
    let pid = cpu::CPU.lock().current_pid().expect("no process");
    let mut process_table = thread::uprocess::PROCESS_TABLE.lock();

    if process_table[pid].is_none() {
        return Err("no process");
    }

    let address = process_table[pid].unwrap().heap_size as u64;
    if let Err(e) = grow_process_heap(n, &mut process_table[pid].unwrap()) {
        return Err(e);
    }
    Ok(address)
}
