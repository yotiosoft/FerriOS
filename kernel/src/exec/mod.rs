use alloc::vec::Vec;
use core::{ cmp, mem::size_of, ptr };
use x86_64::{ PhysAddr, VirtAddr, registers::control, structures::paging::{ FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, frame, page } };

use crate::{cpu, memory::va};
use crate::memory;
use crate::thread;
use crate::thread::uprocess::USER_STACK_TOP;

pub mod user_programs;

const ELF_MAGIC_NUM: u32 = 0x464C457F;
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LE: u8 = 1;
const ELF_TYPE_EXEC: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 0x3E;
const ELF_PROG_LOAD: u32 = 1;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Elf64Header {
    ident: [u8; 16],
    elf_type: u16,
    machine: u16,
    version: u32,
    entry: u64,
    phoff: u64,
    shoff: u64,
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    phnum: u16,
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

impl Elf64Header {
    /// ELF 識別子先頭4バイトを magic 値として返す
    fn magic(&self) -> u32 {
        u32::from_le_bytes([self.ident[0], self.ident[1], self.ident[2], self.ident[3]])
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Elf64ProgramHeader {
    prog_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

pub struct Exec {
    pub page_table: PhysFrame,
    pub entry: u64,
    pub user_sp: u64,
    pub argc: usize,
    pub argv_user_ptr: u64,
    pub heap_size: usize,
}

/// パスからユーザプログラムを解決して exec を完了する
pub fn exec(path: &str, argv: &[Vec<u8>]) -> Result<(), &'static str> {
    let elf_image = user_programs::lookup(path).ok_or("exec: program not found")?;
    let prepared = prepare_exec_image(elf_image, &argv)?;
    commit_exec(prepared)?;
    Ok(())
}

/// ELF と argv から新しい実行イメージを準備する
pub fn prepare_exec_image(elf_image: &[u8], argv: &[Vec<u8>]) -> Result<Exec, &'static str> {
    if elf_image.len() < size_of::<Elf64Header>() {
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
    let heap_size = load_elf_segments(elf_image, elf, pml4, &mut user_mapper, frame_allocator)?;

    // ユーザスタック範囲を計算し、先頭側にガードページを置く
    let stack_top = USER_STACK_TOP;
    let stack_pages = memory::STACK_PAGES as u64;
    let stack_bytes = stack_pages
        .checked_mul(memory::PAGE_SIZE as u64)
        .ok_or("exec: stack size overflow")?;
    let stack_start = stack_top.checked_sub(stack_bytes).ok_or("exec: stack overflow")?;
    let guard_page_start = stack_start
        .checked_sub(memory::PAGE_SIZE as u64)
        .ok_or("exec: guard page overflow")?;

    let guard_page = Page::containing_address(VirtAddr::new(guard_page_start));
    let stack_start_page = Page::containing_address(VirtAddr::new(stack_start));
    // ガードページ（U=0）を1枚確保
    memory::va::map_page(
        &mut user_mapper,
        frame_allocator,
        guard_page,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    )?;
    // 実際のユーザスタックを複数ページで確保
    memory::va::map_pages(
        &mut user_mapper,
        frame_allocator,
        stack_start_page,
        stack_pages,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
    )?;

    let (user_sp, argc, argv_user_ptr) = setup_user_stack(pml4, frame_allocator, argv, stack_top)?;

    Ok(Exec {
        page_table,
        entry: elf.entry,
        user_sp,
        argc,
        argv_user_ptr,
        heap_size
    })
}

/// ELF ヘッダを読み取り、対象アーキテクチャなどを検証する
fn read_elf_header(image: &[u8]) -> Result<Elf64Header, &'static str> {
    if image.len() < size_of::<Elf64Header>() {
        return Err("exec: ELF header is truncated");
    }

    let elf = unsafe {
        ptr::read_unaligned(image.as_ptr() as *const Elf64Header)
    };

    if elf.ident[0..4] != [0x7F, b'E', b'L', b'F'] {
        return Err("exec: bad ELF magic");
    }
    if elf.ident[4] != ELF_CLASS_64 || elf.ident[5] != ELF_DATA_LE {
        return Err("exec: unsupported ELF class");
    }
    if elf.elf_type != ELF_TYPE_EXEC || elf.machine != ELF_MACHINE_X86_64 || elf.version != 1 {
        return Err("exec: unsupported ELF target");
    }
    if elf.magic() != ELF_MAGIC_NUM {
        return Err("exec: bad ELF magic");
    }

    Ok(elf)
}

/// LOAD セグメントをユーザページテーブルへマップして内容を配置する
fn load_elf_segments(image: &[u8], elf: Elf64Header, pml4: &mut PageTable, user_mapper: &mut OffsetPageTable<'static>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<usize, &'static str> {
    let pa_offset = usize::try_from(elf.phoff).map_err(|_| "exec: invalid phoff")?;
    let pa_entry_size = usize::from(elf.phentsize);
    if pa_entry_size != size_of::<Elf64ProgramHeader>() {
        return Err("exec: unexpected program header size");
    }

    let mut max_load_end = 0u64;

    for i in 0..usize::from(elf.phnum) {
        let start = pa_offset.checked_add(i.checked_mul(pa_entry_size).ok_or("exec: program header overflow")?).ok_or("exec: program header overflow")?;
        let end = start.checked_add(pa_entry_size).ok_or("exec: program header overflow")?;
        if end > image.len() {
            return Err("exec: truncated program header");
        }

        let program_header = unsafe {
            ptr::read_unaligned(image[start..end].as_ptr() as *const Elf64ProgramHeader)
        };
        if program_header.prog_type != ELF_PROG_LOAD {
            continue;
        }
        crate::debug!(
            "[exec] LOAD vaddr={:#x}, filesz={:#x}, memsz={:#x}, flags={:#x}",
            program_header.vaddr,
            program_header.filesz,
            program_header.memsz,
            program_header.flags,
        );
        if program_header.memsz < program_header.filesz {
            return Err("exec: invalid LOAD segment sizes");
        }
        if program_header.memsz == 0 {
            continue;
        }

        let file_start = usize::try_from(program_header.offset).map_err(|_| "exec: invalid segment offset")?;
        let file_size = usize::try_from(program_header.filesz).map_err(|_| "exec: invalid segment size")?;
        let file_end = file_start.checked_add(file_size).ok_or("exec: segment overflow")?;
        if file_end > image.len() {
            return Err("exec: truncated LOAD segment");
        }

        let segment_start = VirtAddr::new(program_header.vaddr);
        let segment_end_u64 = program_header.vaddr.checked_add(program_header.memsz).ok_or("exec: invalid segment address")?;
        let segment_end = VirtAddr::new(segment_end_u64);
        max_load_end = cmp::max(max_load_end, segment_end_u64);

        let start_page = Page::containing_address(segment_start);
        let end_page = Page::containing_address(segment_end - 1u64);
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if (program_header.flags & 0x2) != 0 {
            flags |= PageTableFlags::WRITABLE;
        }

        for page in Page::range_inclusive(start_page, end_page) {
            ensure_user_page_mapping(pml4, user_mapper, frame_allocator, page, flags)?;
        }

        zero_user_range(pml4, frame_allocator, program_header.vaddr, program_header.memsz)?;
        copy_to_user_pagetable(pml4, frame_allocator, program_header.vaddr, &image[file_start..file_end])?;
    }

    pg_round_up(max_load_end)
}

fn pg_round_up(value: u64) -> Result<usize, &'static str> {
    let page_size = memory::PAGE_SIZE as u64;
    let rounded = value
        .checked_add(page_size - 1)
        .ok_or("exec: heap size overflow")?
        & !(page_size - 1);

    usize::try_from(rounded).map_err(|_| "exec: heap size overflow")
}

fn ensure_user_page_mapping(
    pml4: &mut PageTable,
    user_mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    page: Page,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET
        .lock()
        .expect("PHYSICAL_MEMORY_OFFSET not initialized");
    let pte = unsafe {
        memory::va::walk_pagetable(
            pml4,
            page.start_address(),
            false,
            physical_memory_offset,
            frame_allocator,
        )
    };

    if let Some(entry) = pte {
        if entry.flags().contains(PageTableFlags::PRESENT) {
            entry.set_flags(entry.flags() | flags);
            return Ok(());
        }
    }

    memory::va::map_page(user_mapper, frame_allocator, page, flags)
}

/// 初期ユーザスタックを構築し、argc/argv の配置先を返す
fn setup_user_stack(pml4: &mut PageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>, argv: &[Vec<u8>], stack_top: u64) -> Result<(u64, usize, u64), &'static str> {
    let mut sp = stack_top;
    let mut argv_ptrs = Vec::new();

    for arg in argv {
        let arg_len = u64::try_from(arg.len() + 1).map_err(|_| "exec: argument overflow")?;
        sp = (sp.checked_sub(arg_len).ok_or("exec: stack overflow")?) & !0x7;

        copy_to_user_pagetable(pml4, frame_allocator, sp, arg)?;
        copy_to_user_pagetable(pml4, frame_allocator, sp + arg.len() as u64, &[0])?;
        argv_ptrs.push(sp);
    }
    argv_ptrs.push(0);

    let argv_bytes = argv_ptrs.len().checked_mul(size_of::<u64>()).ok_or("exec: argv overflow")?;
    sp = (sp.checked_sub(u64::try_from(argv_bytes).map_err(|_| "exec: argv overflow")?).ok_or("exec: stack overflow")?) & !0xF;
    let argv_user_ptr = sp;
    copy_u64_slice_to_user(pml4, frame_allocator, argv_user_ptr, &argv_ptrs)?;

    let ustack = [0xFFFF_FFFF_FFFF_FFFF, argv.len() as u64, argv_user_ptr];
    sp = sp.checked_sub(u64::try_from(ustack.len() * size_of::<u64>()).map_err(|_| "exec: stack overflow")?).ok_or("exec: stack overflow")?;
    copy_u64_slice_to_user(pml4, frame_allocator, sp, &ustack)?;

    Ok((sp, argv.len(), argv_user_ptr))
}

/// `u64` 配列をリトルエンディアン列にしてユーザ空間へコピーする
fn copy_u64_slice_to_user(pml4: &mut PageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>, dst: u64, data: &[u64]) -> Result<(), &'static str> {
    let mut bytes = Vec::with_capacity(data.len() * size_of::<u64>());
    for value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    copy_to_user_pagetable(pml4, frame_allocator, dst, &bytes)
}

/// 現在構築中のユーザページテーブルに任意バイト列を書き込む
fn copy_to_user_pagetable(pml4: &mut PageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>, dst: u64, src: &[u8]) -> Result<(), &'static str> {
    let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET.lock().expect("PHYSICAL_MEMORY_OFFSET not initialized");
    let mut written = 0 as usize;

    while written < src.len() {
        let va = VirtAddr::new(dst + written as u64);
        let page_offset = usize::from(va.page_offset());
        let to_copy = cmp::min(memory::PAGE_SIZE - page_offset, src.len() - written);
        let pte = unsafe {
            memory::va::walk_pagetable(pml4, va, false, physical_memory_offset, frame_allocator)
        }.ok_or("exec: address is not mapped")?;
        if !pte.flags().contains(PageTableFlags::PRESENT) {
            return Err("exec: address is not present");
        }

        let page_va = unsafe {
            memory::va::phys_to_virt(PhysAddr::new(pte.addr().as_u64()), physical_memory_offset)
        };
        unsafe {
            ptr::copy_nonoverlapping(src[written..(written + to_copy)].as_ptr(), page_va.as_mut_ptr::<u8>().add(page_offset), to_copy);
        }
        written += to_copy;
    }

    Ok(())
}

/// ユーザ空間の指定範囲をゼロクリアする
fn zero_user_range(pml4: &mut PageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>, start: u64, len: u64) -> Result<(), &'static str> {
    let physical_memory_offset = memory::PHYSICAL_MEMORY_OFFSET.lock().expect("PHYSICAL_MEMORY_OFFSET not initialized");
    let mut cleared = 0 as u64;

    while cleared < len {
        let va = VirtAddr::new(start + cleared);
        let page_offset = usize::from(va.page_offset());
        let to_zero = cmp::min(memory::PAGE_SIZE - page_offset, usize::try_from(len - cleared).unwrap_or(usize::MAX));
        let pte = unsafe {
            memory::va::walk_pagetable(pml4, va, false, physical_memory_offset, frame_allocator)
        }.ok_or("exec: address is not mapped")?;
        if !pte.flags().contains(PageTableFlags::PRESENT) {
            return Err("exec: address is not present");
        }

        let page_va = unsafe {
            memory::va::phys_to_virt(PhysAddr::new(pte.addr().as_u64()), physical_memory_offset)
        };
        unsafe {
            ptr::write_bytes(page_va.as_mut_ptr::<u8>().add(page_offset), 0, to_zero);
        }
        cleared += to_zero as u64;
    }

    Ok(())
}

/// 準備済み実行イメージを現在プロセス/スレッドへ反映する
fn commit_exec(prepared: Exec) -> Result<(), &'static str> {
    let (current_tid, current_pid) = {
        let cpu = cpu::CPU.lock();
        (
            cpu.current_tid().ok_or("exec: no current thread id")?,
            cpu.current_pid().ok_or("exec: no current process id")?,
        )
    };

    // ロールバック用のスナップショットを作成しておく
    let (old_page_table, old_heap_size) = {
        let process_table = thread::uprocess::PROCESS_TABLE.lock();
        let process = process_table
            .get(current_pid)
            .and_then(|p| p.as_ref())
            .ok_or("exec: process table entry missing")?;
        (process.page_table, process.heap_size)
    };

    let (old_context, old_trap_frame, old_saved_user_rsp) = {
        let thread_table = thread::THREAD_TABLE.lock();
        let thread = thread_table
            .get(current_tid)
            .ok_or("exec: thread table entry missing")?;
        let trap_frame = thread.tf.ok_or("exec: no trapframe")?;
        let old_tf = unsafe { *trap_frame };
        let old_context = thread.context;
        drop(thread_table);

        let cpu = cpu::CPU.lock();
        (old_context, old_tf, cpu.saved_user_rsp)
    };

    let (old_cr3, old_cr3_flags) = control::Cr3::read();

    // 反映本体（途中で失敗したら下でロールバックする）
    let commit_result = (|| -> Result<(), &'static str> {
        {
            let mut process_table = thread::uprocess::PROCESS_TABLE.lock();
            let process = process_table
                .get_mut(current_pid)
                .and_then(|p| p.as_mut())
                .ok_or("exec: process table entry missing")?;
            process.page_table = Some(prepared.page_table);
            process.heap_size = prepared.heap_size;
        }

        {
            let mut thread_table = thread::THREAD_TABLE.lock();
            let thread = thread_table
                .get_mut(current_tid)
                .ok_or("exec: thread table entry missing")?;
            thread.context.rsp3 = prepared.user_sp;
            thread.context.user_rip = prepared.entry;
            thread.context.user_rdi = prepared.argc as u64;
            thread.context.user_rsi = prepared.argv_user_ptr;

            let trap_frame = thread.tf.ok_or("exec: no trapframe")?;
            unsafe {
                (*trap_frame).rax = 0;
                (*trap_frame).rdi = prepared.argc as u64;
                (*trap_frame).rsi = prepared.argv_user_ptr;
                (*trap_frame).rcx = prepared.entry;
            }
        }

        {
            let mut cpu = cpu::CPU.lock();
            cpu.saved_user_rsp = prepared.user_sp;
        }

        unsafe {
            control::Cr3::write(prepared.page_table, control::Cr3Flags::empty());
        }
        Ok(())
    })();

    if commit_result.is_err() {
        // 途中失敗時はロールバック
        {
            let mut process_table = thread::uprocess::PROCESS_TABLE.lock();
            if let Some(process) = process_table.get_mut(current_pid).and_then(|p| p.as_mut()) {
                process.page_table = old_page_table;
                process.heap_size = old_heap_size;
            }
        }

        {
            let mut thread_table = thread::THREAD_TABLE.lock();
            if let Some(thread) = thread_table.get_mut(current_tid) {
                thread.context = old_context;
                if let Some(trap_frame) = thread.tf {
                    unsafe {
                        *trap_frame = old_trap_frame;
                    }
                }
            }
        }

        {
            let mut cpu = cpu::CPU.lock();
            cpu.saved_user_rsp = old_saved_user_rsp;
        }

        unsafe {
            control::Cr3::write(old_cr3, old_cr3_flags);
        }
    }

    commit_result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_elf_header() -> Elf64Header {
        let mut ident = [0u8; 16];
        ident[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        ident[4] = ELF_CLASS_64;
        ident[5] = ELF_DATA_LE;

        Elf64Header {
            ident,
            elf_type: ELF_TYPE_EXEC,
            machine: ELF_MACHINE_X86_64,
            version: 1,
            entry: 0x401000,
            phoff: core::mem::size_of::<Elf64Header>() as u64,
            shoff: 0,
            flags: 0,
            ehsize: core::mem::size_of::<Elf64Header>() as u16,
            phentsize: core::mem::size_of::<Elf64ProgramHeader>() as u16,
            phnum: 0,
            shentsize: 0,
            shnum: 0,
            shstrndx: 0,
        }
    }

    fn header_bytes(header: &Elf64Header) -> [u8; core::mem::size_of::<Elf64Header>()] {
        let mut bytes = [0u8; core::mem::size_of::<Elf64Header>()];

        unsafe {
            core::ptr::copy_nonoverlapping(
                header as *const Elf64Header as *const u8,
                bytes.as_mut_ptr(),
                bytes.len(),
            );
        }

        bytes
    }

    #[test_case]
    fn read_elf_header_accepts_valid_x86_64_exec_header() {
        let bytes = header_bytes(&valid_elf_header());
        let elf = read_elf_header(&bytes).expect("valid ELF header");

        assert_eq!(elf.magic(), ELF_MAGIC_NUM);
        assert_eq!(elf.entry, 0x401000);
        assert_eq!(elf.phoff, core::mem::size_of::<Elf64Header>() as u64);
        assert_eq!(
            elf.phentsize,
            core::mem::size_of::<Elf64ProgramHeader>() as u16
        );
    }

    #[test_case]
    fn read_elf_header_rejects_truncated_header() {
        let bytes = [0u8; core::mem::size_of::<Elf64Header>() - 1];

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: ELF header is truncated")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_bad_magic() {
        let mut header = valid_elf_header();
        header.ident[0] = 0;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: bad ELF magic")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_unsupported_class() {
        let mut header = valid_elf_header();
        header.ident[4] = 1;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: unsupported ELF class")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_unsupported_endianness() {
        let mut header = valid_elf_header();
        header.ident[5] = 2;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: unsupported ELF class")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_non_executable_type() {
        let mut header = valid_elf_header();
        header.elf_type = 1;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: unsupported ELF target")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_non_x86_64_machine() {
        let mut header = valid_elf_header();
        header.machine = 0x28;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: unsupported ELF target")
        );
    }

    #[test_case]
    fn read_elf_header_rejects_unsupported_version() {
        let mut header = valid_elf_header();
        header.version = 0;
        let bytes = header_bytes(&header);

        assert_eq!(
            read_elf_header(&bytes).map(|_| ()),
            Err("exec: unsupported ELF target")
        );
    }

    #[test_case]
    fn pg_round_up_handles_page_boundaries() {
        assert_eq!(pg_round_up(0), Ok(0));
        assert_eq!(pg_round_up(memory::PAGE_SIZE as u64 - 1), Ok(memory::PAGE_SIZE));
        assert_eq!(pg_round_up(memory::PAGE_SIZE as u64), Ok(memory::PAGE_SIZE));
        assert_eq!(
            pg_round_up(memory::PAGE_SIZE as u64 + 1),
            Ok(memory::PAGE_SIZE * 2)
        );
    }

    #[test_case]
    fn pg_round_up_reports_overflow() {
        assert_eq!(pg_round_up(u64::MAX), Err("exec: heap size overflow"));
    }

    #[test_case]
    fn exec_unknown_path_returns_not_found() {
        assert_eq!(exec("/missing", &[]), Err("exec: program not found"));
    }
}
