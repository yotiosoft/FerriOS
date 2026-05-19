#![no_std]

/// type: syscall number
pub type SyscallNum = i64;

/// type: return value
pub type SysRet = i64;

/// syscall numbers
pub const SYS_PRINT_NUM: SyscallNum = 1;
pub const SYS_PRINT_STR: SyscallNum = 2;
pub const SYS_FORK: SyscallNum = 3;
pub const SYS_EXEC: SyscallNum = 4;
pub const SYS_GETPID: SyscallNum = 5;

/// return values
pub const RET_SUCCESS: SysRet = 0;
pub const RET_ERROR: SysRet = -1;
