#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};
pub use abi::*;

mod entry;
pub use entry::*;

mod syscalls;
pub use syscalls::*;

mod printfmt;
pub use printfmt::*;

mod panic;
pub use panic::*;

const EXEC_MAX_ARGC: usize = 16;
const EXEC_MAX_ARG_LEN: usize = 256;
const PRINT_FMT_BUF_LEN: usize = 256;

fn copy_c_string(src: &str, dst: &mut [u8; EXEC_MAX_ARG_LEN + 1]) -> Result<(), ()> {
    let bytes = src.as_bytes();
    if bytes.len() > EXEC_MAX_ARG_LEN {
        return Err(());
    }

    dst[..bytes.len()].copy_from_slice(bytes);
    dst[bytes.len()] = 0;
    Ok(())
}
