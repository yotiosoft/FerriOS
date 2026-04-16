#![no_std]

/// syscall numbers
pub const SYS_PRINT_NUM: u64 = 1;
pub const SYS_PRINT_STR: u64 = 2;
pub const SYS_FORK: u64 = 3;
pub const SYS_EXEC: u64 = 4;

/// return values
pub const RET_SUCCESS: u64 = 0;
pub const RET_ERROR: u64 = 1;
