use super::*;
use core::ffi::{c_char, CStr};

pub static mut ARGC: usize = 0;
pub static mut ARGV: *const *const u8 = core::ptr::null();

fn argc() -> usize {
    unsafe { ARGC }
}

fn argv() -> *const *const u8 {
    unsafe { ARGV }
}

pub fn args() -> Args {
    Args {
        argc: argc(),
        argv: argv(),
        index: 0,
    }
}

pub struct Args {
    argc: usize,
    argv: *const *const u8,
    index: usize,
}

impl Iterator for Args {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.argc || self.argv.is_null() {
            return None;
        }

        let arg_ptr = unsafe { *self.argv.add(self.index) };
        self.index += 1;

        if arg_ptr.is_null() {
            return None;
        }

        let arg = unsafe {
            CStr::from_ptr(arg_ptr.cast::<c_char>())
        };
        Some(arg.to_str().unwrap_or(""))
    }
}
