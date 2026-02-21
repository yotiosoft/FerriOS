use x86_64::{ VirtAddr, structures::paging::{ FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB } };

// ユーザコードのアドレス
pub const USER_CODE_START: u64 = 0x0000_1000_0000_0000;

// ユーザスタック
pub const USER_STACK_TOP: u64 = 0x000_2000_0000_0000;
