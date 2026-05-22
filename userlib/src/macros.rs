use super::*;

#[macro_export]
macro_rules! execl {
    ($path:expr $(, $arg:expr)* $(,)?) => {{
        $crate::exec($path, &[$($arg),*])
    }};
}
