use alloc::vec::Vec;
use core::{ cmp, mem::size_of, ptr };
use x86_64::{ PhysAddr, VirtAddr, registers::control, structures::paging::{ FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB } };

use crate::cpu;
use crate::memory;
use crate::thread;
use crate::thread::uprocess::USER_STACK_TOP;

mod elf;
mod user_programs;

const MAX_ARGC: usize = 16;
const MAX_ARG_LEN: usize = 256;
const ELF_MAGIC_NUM: u32 = 0x464C457F;
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LE: u8 = 1;
const ELF_TYPE_EXEC: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 0x3E;
const ELF_PROG_LOAD: u32 = 1;
const USER_STACK_PAGES: u64 = 2;

pub struct Exec {
    pub page_table: PhysFrame,
    pub entry: u64,
    pub user_sp: u64,
    pub argc: usize,
    pub argv_user_ptr: u64,
}

pub fn exec(path: &str, argv: &[Vec<u8>]) -> Result<(), &'static str> {
    let elf_image = user_programs::lookup(path).ok_or("exec: program not found")?;
    let prepared = prepare_exec_image(elf_image, &argv);
    commit_exec(prepared)?;

    

    Ok(())
}
