#![no_std]

use core::alloc;

use userlib::*;

fn main() {
    // 1st child
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

    // 2nd child
    let alloced_memory = sbrk(1234);
    let addr = alloced_memory as *mut u64;
    unsafe {
        *addr = 0;
    }

    let ret = fork();
    let start = uptime();
    if ret == RET_ERROR {
        panic!("failed to call fork()");
    }
    if ret == 0 {
        // on the child process
        unsafe {
            *addr = 0;
        }
        loop {
            unsafe {
                *addr += 1;
                print_fmt!("[child (pid = {})] *addr = {}", getpid(), *addr);
            }
        }
    }

    // on the parent process
    // wait a moment..
    loop {
        if uptime() - start > 5 {
            break;
        }
    }

    // kill it
    print_fmt!("[parent] pid = {} kill pid {}", pid, ret);
    kill(ret as ProcessID);
    print_fmt!("[parent] pid = {} done.", pid);

    // sbrk test
    if alloced_memory == RET_ERROR {
        print_fmt!("[parent] pid = {} alloc failed", pid);
    }
    else {
        unsafe {
            print_fmt!("[parent (pid = {})] *addr = {}", pid, *addr);
        }
    }

    loop {
        //print_fmt!("[parent] pid = {} ticks = {}", pid, uptime());
    }
}
