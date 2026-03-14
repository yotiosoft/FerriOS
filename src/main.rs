#![no_std]      // std ライブラリを使わない
#![no_main]     // main 関数を使わない

#![feature(custom_test_frameworks)] 
#![test_runner(ferrios::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use ferrios::task::keyboard;
use bootloader_api::{ BootInfo, entry_point };
use bootloader_api::config::{BootloaderConfig, Mapping};
use ferrios::task::serial_input;
use core::panic::PanicInfo;
use alloc::{ boxed::Box, vec, vec::Vec, rc::Rc };

use ferrios::{ println, print };
use ferrios::memory;
use ferrios::allocator;
use ferrios::task::{ Task, executor::Executor };
use ferrios::thread;
use ferrios::scheduler;
use ferrios::console;

static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.mappings.kernel_base = Mapping::FixedAddress(0xFFFF_8000_0000_0000); // index 256以上
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// エントリポイント
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    use ferrios::memory::BootInfoFrameAllocator;
    use x86_64::{ structures::paging::Page, structures::paging::Translate, VirtAddr };

    println!("Welcome to FerriOS!");
    println!("");

    print!("Initializing..");
    println!("step 1: before init");
    ferrios::init();
    println!("step 2: after init");
    println!("step 3: before console init");
    console::init();
    println!("step 4: after console init");
    scheduler::init(Box::new(scheduler::round_robin::RoundRobin));
    println!("done.");
    
    let console_mode = console::CONSOLE.lock().get();
    println!("console-mode: {:?}", console_mode);

    println!("Checking Virtual Memory..");
    let phys_mem_offset = VirtAddr::new(
        boot_info.physical_memory_offset.into_option().unwrap()
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_regions)
    };

    // allocator 初期化
    println!("Initializing heap memory..");
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // allocates
    let x = Box::new(41);
    println!("\theap_value at {:p}", x);
    let mut vec = Vec::new();
    for i in 0..500 {
        vec.push(i);
    }
    println!("\tvec at {:p}", vec.as_slice());
    // 参照されたベクタを作成する → カウントが0になると解放される
    let reference_counted = Rc::new(vec![1, 2, 3]);
    let cloned_reference = reference_counted.clone();
    println!("\tcurrent reference count is {}", Rc::strong_count(&cloned_reference));
    core::mem::drop(reference_counted);
    println!("\treference count is {} now", Rc::strong_count(&cloned_reference));
    println!("done.");

    #[cfg(test)]
    test_main();
    
    // カーネルスレッド作成
    print!("Starting kernel threads..");
    thread::kthread::create_kernel_thread(kernel_thread_0);
    thread::kthread::create_kernel_thread(kernel_thread_1);
    thread::kthread::create_kernel_thread(kernel_thread_2);
    thread::kthread::create_kernel_thread(keyboard_and_serial_input_thread);
    println!("done.");

    // ユーザプロセス作成
    const USER_CODE: &[u8] = &[
        0x48, 0x31, 0xC0,           // xor rax, rax
        0x48, 0xFF, 0xC0,           // inc rax
        0xEB, 0xFB,                 // jmp -5
    ];

    thread::uprocess::create_user_process(USER_CODE, &mut mapper, &mut frame_allocator).expect("failed to create user process");

    println!("Starting the scheduler..");
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
fn kernel_thread_2() -> ! {
    let mut count = 0;
    loop {
        // 割り込みが有効か確認
        println!("Thread 2 running: {}", count);
        count = count + 1;
        
        for _ in 0..1000000 {
            unsafe { core::arch::asm!("nop"); }
        }
    }
}

// キーボード＆シリアル割り込み用スレッド
fn keyboard_and_serial_input_thread() -> ! {
    let mut executor = Executor::new();
    executor.spawn(Task::new(keyboard::print_keypresses()));
    executor.spawn(Task::new(serial_input::thread_serial_input()));
    executor.run();
}

/// パニックハンドラ
/// パニック時に呼ばれる
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    ferrios::hlt_loop();
}

/// テスト時に使うパニックハンドラ
#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    ferrios::test_panic_handler(info)
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
