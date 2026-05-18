#![no_std]

use core::ops::Add;

/// type: syscall number
pub type SyscallNum = i64;

/// type: return value
pub type RetValue = i64;
pub type SysRet = RetValue;

/// type: Address
pub type Address = u64;
pub type KernelAddress = Address;
pub type UserAddress = Address;

/// type: process id
pub type ProcessID = usize;

/// type: thread id
pub type ThreadID = usize;

/// syscall numbers
pub const SYS_PRINT_NUM: SyscallNum = 1;
pub const SYS_PRINT_STR: SyscallNum = 2;
pub const SYS_FORK: SyscallNum = 3;
pub const SYS_EXEC: SyscallNum = 4;
pub const SYS_GETPID: SyscallNum = 5;
pub const SYS_UPTIME: SyscallNum = 6;
pub const SYS_EXIT: SyscallNum = 7;
pub const SYS_WAIT: SyscallNum = 8;

/// return values
pub const RET_SUCCESS: SysRet = 0;
pub const RET_ERROR: SysRet = -1;

/// pointer
pub const NULL_POINTER: Address = 0;
