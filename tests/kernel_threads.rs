#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(ferrios::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use ferrios::println;
use ferrios::thread;
use ferrios::scheduler;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    use ferrios::allocator;
    use ferrios::memory::{self, BootInfoFrameAllocator};
    use x86_64::VirtAddr;

    ferrios::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe {
        memory::init(phys_mem_offset)
    };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // カーネルスレッド作成
    thread::create_kernel_thread(kernel_thread_0);
    thread::create_kernel_thread(kernel_thread_1);

    scheduler::scheduler();
}

// カーネルスレッド
fn kernel_thread_0() -> ! {
    let mut count = 0;
    loop {
        // 割り込みが有効か確認
        println!("Thread 0 running: {}", count);
        count = count + 1;
        
        for _ in 0..1000000 {
            unsafe { core::arch::asm!("nop"); }
        }
    }
}
fn kernel_thread_1() -> ! {
    let mut count = 0;
    loop {
        // 割り込みが有効か確認
        println!("Thread 1 running: {}", count);
        count = count + 1;
        
        for _ in 0..1000000 {
            unsafe { core::arch::asm!("nop"); }
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    ferrios::test_panic_handler(info)
}
