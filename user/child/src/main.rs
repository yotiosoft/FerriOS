#![no_std]
#![no_main]

use userlib::*;

userlib::entry!(main);

fn main() -> RetValue {
    let pid = getpid();
    let mut ret = 0;
    for _ in 0..60 {
        ret += uptime();
        print_fmt!("[child] pid = {} ticks = {} ret = {}", pid, uptime(), ret);
    }
    print_fmt!("[child] exiting..");

    ret
}
