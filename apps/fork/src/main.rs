#![no_std]
#![no_main]

use core::panic::PanicInfo;
use userlib::*;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let ret = fork();
    if ret == 0 {
        panic!("failed to call fork()");
    }

    let pid = getpid();
    print_num(pid);

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
