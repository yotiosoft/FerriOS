use super::*;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    print_fmt!("[Panic Handler] A user panic occured! (pid {})\n{}", getpid(), info);

    exit(abi::RET_ERROR);
}
