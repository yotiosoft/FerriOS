use alloc::vec::Vec;
use core::{ cmp, mem::size_of, ptr };
use x86_64::{ PhysAddr, VirtAddr, registers::control, structures::paging::{ FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, frame, page } };

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

fn prepare_exec_image(elf_image: &[u8], argv: &[Vec<u8>]) -> Result<Exec, &'static str> {
    if elf_image.len() < size_of::<elf::Elf64Header>() {
        return Err("exec: invalid ELF image");
    }

    let elf = read_elf_header(elf_image)?;

    let mut guard = memory::FRAME_ALLOCATOR.lock();
    let frame_allocator = guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let (mut user_mapper, page_table) = memory::umem::new_uvm(frame_allocator)?;

    let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET.lock().expect("PHYSICAL_MEMORY_OFFSET not initialized");
    let pml4 = unsafe {
        &mut *(memory::va::phys_to_virt(page_table.start_address(), physical_memory_offset).as_mut_ptr::<PageTable>())
    };
    load_elf_segments(elf_image, &elf, pml4, &mut user_mapper, frame_allocator)?;

    let stack_top = USER_STACK_TOP;
    let guard_page = Page::containing_address(VirtAddr::new(stack_top - USER_STACK_PAGES * memory::PAGE_SIZE as u64));
    let stack_page = Page::containing_address(VirtAddr::new(stack_top - memory::PAGE_SIZE as u64));
    memory::va::map_page(&mut user_mapper, frame_allocator, guard_page, PageTableFlags::PRESENT | PageTableFlags::WRITABLE)?;
    memory::va::map_page(&mut user_mapper, frame_allocator, stack_page, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE)?;

    let (user_sp, argc, argv_user_ptr) = setup_user_stack(pml4, frame_allocator, argv, stack_top)?;

    Ok(Exec {
        page_table,
        entry: elf.entry,
        user_sp,
        argc,
        argv_user_ptr
    })
}
