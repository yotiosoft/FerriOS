use super::*;

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

#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
            let _ = (argc, argv);
            let ret = $crate::Termination::report($main());
            $crate::exit(ret);
        }
    };
}
