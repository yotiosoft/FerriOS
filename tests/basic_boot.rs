#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(ferrios::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use ferrios::println;
use bootloader_api::{entry_point, BootInfo};

entry_point!(kernel_main, config = &ferrios::BOOTLOADER_CONFIG);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    test_main();
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    ferrios::test_panic_handler(info)
}

#[test_case]
fn test_println() {
    println!("test_println output");
}
