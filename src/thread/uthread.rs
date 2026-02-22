use x86_64::{ VirtAddr, structures::paging::{ FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB } };
use crate::gdt;

use super::{ STACK_SIZE, THREAD_TABLE, ThreadState };

/// ユーザコード
pub const USER_CODE_START: u64 = 0x0000_1000_0000_0000;

/// ユーザスタック
pub const USER_STACK_TOP: u64 = 0x0000_2000_0000_0000;
pub const USER_STACK_PAGES: u64 = 4;

pub fn create_user_process(code: &[u8], mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<(), &'static str> {
    // スレッド ID を確保
    let tid = super::next_tid().expect("Thread table is full");

    // ユーザページのフラグ
    let user_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    // コードページ用領域を用意
    let code_page = Page::containing_address(VirtAddr::new(USER_CODE_START));
    let code_frame = frame_allocator.allocate_frame().expect("frame alloc failed");
    
    // コードページにユーザコードをコピー
    unsafe {
        mapper.map_to(code_page, code_frame, user_flags, frame_allocator).map_err(|_| "code map_to failed")?.flush();
        core::ptr::copy_nonoverlapping(code.as_ptr(), USER_CODE_START as *mut u8, code.len());
    }

    // ユーザスタック用領域を用意
    let stack_start = USER_STACK_TOP - USER_STACK_PAGES * 4096;
    for i in 0..USER_STACK_PAGES {
        let page = Page::containing_address(VirtAddr::new(stack_start + i * 4096));
        let frame = frame_allocator.allocate_frame().ok_or("frame alloc failed")?;
        unsafe {
            mapper.map_to(page, frame, user_flags, frame_allocator).map_err(|_| "stack map_to failed")?.flush();
        }
    }

    // カーネルスタックを作成
    let kstack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };
    let kstack_top = kstack as u64 + STACK_SIZE as u64;

    let mut table = THREAD_TABLE.lock();
    table[tid].tid = tid;
    table[tid].state = ThreadState::Runnable;
    table[tid].kstack = kstack_top;

    // コンテキストを初期化する
    table[tid].context.rsp = kstack_top;
    table[tid].context.rip = ring3_entry_trampoline as u64;
    table[tid].context.rflags = 0x200;  // IF (Interrupt Flag) を有効化
    table[tid].context.cs = gdt::GDT.1.user_code_selector.0 as u64;
    table[tid].context.ss = gdt::GDT.1.user_data_selector.0 as u64;
    table[tid].context.rsp3 = USER_STACK_TOP;

    Ok(())
}

unsafe extern "C" fn ring3_entry_trampoline() -> ! {
    let (cs, ss, rsp3, rip) = {
        let table = THREAD_TABLE.lock();
        let ctx =&table[super::current_tid().expect("No running thread")].context;
        (ctx.cs, ctx.ss, ctx.rsp3, USER_CODE_START)
    };

    unsafe {
        core::arch::asm!(
            "mov ds, ax",
            "mov es, ax",
            "push rax",
            "push {rsp3}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            "iretq",
            inout("ax") ss => _,
            cs = in(reg) cs,
            rsp3 = in(reg) rsp3,
            rflags = in(reg) 0x202u64,
            rip = in(reg) rip,
        );
    }

    loop {}
}
