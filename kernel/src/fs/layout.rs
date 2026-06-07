use super::block::BLOCK_SIZE;

pub const ROOT_INO: u32 = 1;
pub const NDIRECT: usize = 10;
pub const NINDIRECT: usize = BLOCK_SIZE / core::mem::size_of::<u32>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT * NINDIRECT * NINDIRECT;
pub const DIRSIZ: usize = 14;
pub const IPB: usize = BLOCK_SIZE / core::mem::size_of::<DiskInode>();
pub const BPB: usize = BLOCK_SIZE * 8;

pub const T_NONE: u16 = 0;
pub const T_DIR: u16 = 1;
pub const T_FILE: u16 = 2;
pub const T_DEV: u16 = 3;

// On-disk values are stored in little-endian order. FerriOS currently targets
// x86_64, so these repr(C) structs match the native in-memory layout.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperBlock {
    pub size: u32,
    pub nblocks: u32,
    pub ninodes: u32,
    pub nlog: u32,
}

impl SuperBlock {
    pub fn inode_blocks(&self) -> u32 {
        self.ninodes / IPB as u32 + 1
    }

    pub fn bitmap_blocks(&self) -> u32 {
        self.size.div_ceil(BPB as u32)
    }

    pub fn inode_start(&self) -> u32 {
        2
    }

    pub fn bitmap_start(&self) -> u32 {
        bitmap_block(0, self.ninodes)
    }

    pub fn data_start(&self) -> u32 {
        self.bitmap_start() + self.bitmap_blocks()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskInode {
    pub type_: u16,
    pub major: u16,
    pub minor: u16,
    pub nlink: u16,
    pub size: u32,
    pub addrs: [u32; NDIRECT + 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirEntry {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

pub const fn inode_block(inum: u32) -> u32 {
    inum / IPB as u32 + 2
}

pub const fn bitmap_block(block_no: u32, ninodes: u32) -> u32 {
    block_no / BPB as u32 + ninodes / IPB as u32 + 3
}

pub const fn inode_index_in_block(inum: u32) -> usize {
    (inum as usize) % IPB
}

pub const fn bitmap_index_in_block(block_no: u32) -> usize {
    ((block_no as usize) % BPB) / 8
}

pub const fn bitmap_mask(block_no: u32) -> u8 {
    1 << ((block_no as usize) % 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn disk_inode_size_check() {
        assert_eq!(core::mem::size_of::<DiskInode>(), 64);
        assert_eq!(core::mem::align_of::<DiskInode>(), 4);
    }

    #[test_case]
    fn dir_entry_size_check() {
        assert_eq!(core::mem::size_of::<DirEntry>(), 16);
        assert_eq!(core::mem::align_of::<DirEntry>(), 2);
    }

    #[test_case]
    fn layout_constants_check() {
        assert_eq!(IPB, BLOCK_SIZE / core::mem::size_of::<DiskInode>());
        assert_eq!(BPB, BLOCK_SIZE * 8);
        assert_eq!(NINDIRECT, 128);
        assert_eq!(MAXFILE, NDIRECT + NINDIRECT * NINDIRECT * NINDIRECT);
    }

    #[test_case]
    fn inode_block_check() {
        assert_eq!(inode_block(ROOT_INO), ROOT_INO / IPB as u32 + 2);
        assert_eq!(inode_block(0), 2);
        assert_eq!(inode_block(IPB as u32), 3);
    }

    #[test_case]
    fn bitmap_block_check() {
        let ninodes = (IPB * 10) as u32;

        assert_eq!(bitmap_block(0, ninodes), ninodes / IPB as u32 + 3);
        assert_eq!(
            bitmap_block(BPB as u32, ninodes),
            BPB as u32 / BPB as u32 + ninodes / IPB as u32 + 3
        );
    }

    #[test_case]
    fn superblock_helpers_check() {
        let superblock = SuperBlock {
            size: BPB as u32 + 1,
            nblocks: 100,
            ninodes: (IPB * 10) as u32,
            nlog: 0,
        };

        assert_eq!(superblock.inode_start(), 2);
        assert_eq!(
            superblock.inode_blocks(),
            superblock.ninodes / IPB as u32 + 1
        );
        assert_eq!(superblock.bitmap_blocks(), 2);
        assert_eq!(
            superblock.bitmap_start(),
            bitmap_block(0, superblock.ninodes)
        );
        assert_eq!(superblock.data_start(), superblock.bitmap_start() + 2);
    }

    #[test_case]
    fn bitmap_helpers_pick_byte_and_bit_inside_bitmap_block() {
        assert_eq!(bitmap_index_in_block(0), 0);
        assert_eq!(bitmap_mask(0), 0x01);
        assert_eq!(bitmap_index_in_block(7), 0);
        assert_eq!(bitmap_mask(7), 0x80);
        assert_eq!(bitmap_index_in_block(8), 1);
        assert_eq!(bitmap_mask(8), 0x01);
    }
}
