#![no_std]

use userlib::*;

fn main() -> RetValue {
    let pid = getpid();

    let args = args();
    for arg in args {
        print_fmt!("[child (pid={})] arg: {}", pid, arg);
    }

    let mut ret = 0;
    for _ in 0..60 {
        ret += uptime();
        print_fmt!("[child (pid={})] ticks = {} ret = {}", pid, uptime(), ret);
    }
    print_fmt!("[child (pid={})] exiting..", pid);

    ret
}
