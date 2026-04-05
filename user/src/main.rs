#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_PRINT_NUM: u64 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop { 
        unsafe {
            asm!(
                "syscall",
                in("rax") SYS_PRINT_NUM,
                in("rdi") 123u64,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
