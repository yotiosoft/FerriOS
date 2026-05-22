#![no_std]

use userlib::*;

fn main() {
    let ret = fork();
    if ret == RET_ERROR {
        panic!("failed to call fork()");
    }
    if ret == 0 {
        // on the child process
        let ret = execl!("/child", "abc", "def");
        if ret == RET_ERROR {
            panic!("failed to call exec()");
        }
    }

    // on the parent process
    let pid = getpid();

    print_fmt!("[parent] waiting child process...");
    let mut status: RetValue = RET_SUCCESS;
    let child_pid = wait(Some(&mut status));
    print_fmt!("[parent (pid={})] child process has exited; child's pid is {} and ret value is {}", pid, child_pid, status);

    loop {
        //print_fmt!("[parent] pid = {} ticks = {}", pid, uptime());
    }
}
