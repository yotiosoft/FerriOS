#![no_std]
#![no_main]

use core::panic::PanicInfo;
use userlib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let pid = getpid();
    for _ in 0..60 {
        print_fmt!("[child] pid = {} ticks = {}", pid, uptime());
    }
    print_fmt!("[child] exiting..");

    exit(123456);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
