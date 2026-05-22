use super::*;

pub use args::*;

#[lang = "termination"]
pub trait Termination {
    fn report(self) -> RetValue;
}

impl Termination for () {
    fn report(self) -> RetValue {
        RET_SUCCESS
    }
}

impl Termination for RetValue {
    fn report(self) -> RetValue {
        self
    }
}

#[lang = "start"]
fn lang_start<T: Termination + 'static>(
    main: fn() -> T,
    argc: isize,
    argv: *const *const u8,
    sigpipe: u8,
) -> isize {
    let _ = sigpipe;

    unsafe {
        args::ARGC = argc as usize;
        args::ARGV = argv;
    }

    let ret = main().report();
    exit(ret);
}
