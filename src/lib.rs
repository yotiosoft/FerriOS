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
use bootloader_api::{ entry_point, BootInfo };
use bootloader_api::config::{BootloaderConfig, Mapping};

#[cfg(test)]
static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.mappings.kernel_base = Mapping::FixedAddress(0xFFFF_8000_0000_0000); // index 256以上
    config
};

#[cfg(test)]
entry_point!(test_kernel_main, config = &crate::BOOTLOADER_CONFIG);

/// test のエントリポイント
#[cfg(test)]
fn test_kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    init();
    test_main();
    hlt_loop();
}
