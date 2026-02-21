#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(naked_functions)]

pub mod interrupts;
pub mod gdt;
pub mod memory;
pub mod allocator;
pub mod task;
pub mod thread;
pub mod cpu;
pub mod console;
pub mod scheduler;

mod libbackend;
pub use libbackend::exit::*;
pub use libbackend::test::*;
pub use libbackend::error_handlers::*;
pub use libbackend::init::*;

extern crate alloc;

#[cfg(test)]
use bootloader::{ entry_point, BootInfo };

#[cfg(test)]
entry_point!(test_kernel_main);

/// test のエントリポイント
#[cfg(test)]
fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    init();
    test_main();
    hlt_loop();
}
