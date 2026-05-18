#![no_std]
#![no_main]

use core::panic::PanicInfo;
use userlib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    let ret = fork();
    if ret == RET_ERROR {
        panic!("failed to call fork()");
    }
    if ret == 0 {
        // on the child process
        let ret = exec("/child", &[]);
        if ret == RET_ERROR {
            panic!("failed to call exec()");
        }
    }

    // on the parent process
    //let pid = getpid();

    print_fmt!("[parent] waiting child process...");
    let mut status: RetValue = RET_SUCCESS;
    let pid = wait(Some(&mut status));
    print_fmt!("[parent] child process has exited; child's pid is {} and ret value is {}", pid, status);

    loop {
        //print_fmt!("[parent] pid = {} ticks = {}", pid, uptime());
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
