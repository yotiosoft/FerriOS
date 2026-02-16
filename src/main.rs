#![no_std]      // std ライブラリを使わない
#![no_main]     // main 関数を使わない

#![feature(custom_test_frameworks)] 
#![test_runner(ferrios::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use ferrios::task::keyboard;
use bootloader::{ BootInfo, entry_point };
use core::panic::PanicInfo;
use alloc::{ boxed::Box, vec, vec::Vec, rc::Rc };

use ferrios::println;
use ferrios::memory;
use ferrios::allocator;
use ferrios::task::{ Task, executor::Executor };
use ferrios::process;

entry_point!(kernel_main);

/// エントリポイント
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use ferrios::memory::BootInfoFrameAllocator;
    use x86_64::{ structures::paging::Page, structures::paging::Translate, VirtAddr };

    {
        use ferrios::serial::SERIAL1;
        SERIAL1.lock();  // これでlazy_staticが初期化される
    }

    println!("Hello World{}", "!");

    ferrios::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };

    // 未使用のページをマップする
    let page = Page::containing_address(VirtAddr::new(0));
    memory::create_example_mapping(page, &mut mapper, &mut frame_allocator);

    // 新しいマッピングを使って文字列 New! を画面に書き出す
    let page_ptr: *mut u64 = page.start_address().as_mut_ptr();
    unsafe {
        page_ptr.offset(400).write_volatile(0x_f021_f077_f065_f04e)
    };

    let addresses = [
        // VGA buffer page
        0xb8000,
        // code page
        0x201008,
        // stack page
        0x0100_0020_1a10,
        // 物理アドレス 0 にマップされている仮想アドレス
        boot_info.physical_memory_offset,
    ];

    for &address in &addresses {
        let virt = VirtAddr::new(address);
        let phys = mapper.translate_addr(virt);
        println!("{:?} -> {:?}", virt, phys);
    }

    // allocator 初期化
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // allocates
    let x = Box::new(41);
    println!("heap_value at {:p}", x);
    let mut vec = Vec::new();
    for i in 0..500 {
        vec.push(i);
    }
    println!("vec at {:p}", vec.as_slice());
    // 参照されたベクタを作成する → カウントが0になると解放される
    let reference_counted = Rc::new(vec![1, 2, 3]);
    let cloned_reference = reference_counted.clone();
    println!("current reference count is {}", Rc::strong_count(&cloned_reference));
    core::mem::drop(reference_counted);
    println!("reference count is {} now", Rc::strong_count(&cloned_reference));

    #[cfg(test)]
    test_main();
    
    println!("It did not crash!");
    
    // カーネルスレッド作成
    process::create_kernel_thread(kernel_thread_0);
    process::create_kernel_thread(kernel_thread_1);
    process::create_kernel_thread(keyboard_thread);

    process::scheduler::scheduler();
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

// キーボード割り込み用スレッド
fn keyboard_thread() -> ! {
    let mut executor = Executor::new();
    executor.spawn(Task::new(keyboard::print_keypresses()));
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
