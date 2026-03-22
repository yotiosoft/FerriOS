use super::exit::*;
use super::super::{ gdt, interrupts, serial_println };
use crate::hlt_loop;
use core::panic::PanicInfo;

extern crate alloc;

/// init
/// IDT の初期化
pub fn init() {
    gdt::init();
    interrupts::init_idt();
    unsafe {
        interrupts::PICS.lock().initialize();
    }
    // IRQ4 のマスクを解除
    unsafe {
        use x86_64::instructions::port::Port;
        let mut port = Port::<u8>::new(0x21);
        let mask = port.read();
        port.write(mask & !(1 << 4));
    }
    x86_64::instructions::interrupts::enable();
}

/// テスト時に使うパニックハンドラ
pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}
