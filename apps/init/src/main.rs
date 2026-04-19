#![no_std]
#![no_main]

use core::panic::PanicInfo;
use userlib::*;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let ret = fork();
    if ret == RET_ERROR {
        panic!("failed to call fork()");
    }
    if ret == 0 {
        loop {}
    }

    let ret = exec("/fork", &[]);
    if ret == RET_ERROR {
        panic!("failed to call exec()");
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
