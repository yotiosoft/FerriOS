#![no_std]
#![no_main]

use core::panic::PanicInfo;
use userlib::*;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop { 
        let ret = print_num(123);
        if ret != 0 {
            panic!("something went wrong..");
        }
        let ret = print_str("Hello!");
        if ret != 0 {
            panic!("something went wrong..");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
