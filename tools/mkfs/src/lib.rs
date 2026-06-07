use std::mem::size_of;

pub const BLOCK_SIZE: usize = 512;
pub const ROOT_INO: u32 = 1;
pub const NDIRECT: usize = 10;
pub const NINDIRECT: usize = BLOCK_SIZE / size_of::<u32>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT;
pub const DIRSIZ: usize = 14;
pub const T_DIR: u16 = 1;
pub const T_FILE: u16 = 2;

const DINODE_SIZE: usize = 64;
const DIRENT_SIZE: usize = 16;
const IPB: u32 = (BLOCK_SIZE / DINODE_SIZE) as u32;
const BPB: u32 = (BLOCK_SIZE * 8) as u32;
const ROOT_DIRENTS: usize = BLOCK_SIZE / DIRENT_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MkfsConfig {
    pub size: u32,
    pub nblocks: u32,
    pub ninodes: u32,
    pub nlog: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkfsError {
    ImageTooSmall,
    NoInodes,
    LayoutOverflow,
    FileNameTooLong,
    DuplicateFileName,
    FileTooLarge,
    RootDirectoryFull,
}

pub fn format_empty_image(config: MkfsConfig) -> Result<Vec<u8>, MkfsError> {
    MkfsBuilder::new(config).build()
}

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MkfsBuilder {
    config: MkfsConfig,
    files: Vec<FileEntry>,
}

impl MkfsBuilder {
    pub fn new(config: MkfsConfig) -> Self {
        Self {
            config,
            files: Vec::new(),
        }
    }

    pub fn add_file<N, C>(&mut self, name: N, content: C) -> Result<&mut Self, MkfsError>
    where
        N: AsRef<str>,
        C: AsRef<[u8]>,
    {
        let name = name.as_ref();
        validate_file_name(name)?;
        if self.files.iter().any(|file| file.name == name) {
            return Err(MkfsError::DuplicateFileName);
        }

        self.files.push(FileEntry {
            name: name.to_owned(),
            content: content.as_ref().to_vec(),
        });
        Ok(self)
    }

    pub fn build(&self) -> Result<Vec<u8>, MkfsError> {
        validate_config(self.config)?;
        validate_files(self.config, &self.files)?;

        let image_len = self
            .config
            .size
            .checked_mul(BLOCK_SIZE as u32)
            .ok_or(MkfsError::LayoutOverflow)? as usize;
        let mut image = vec![0u8; image_len];
        let root_dir_block = data_start(self.config)?;
        let mut next_data_block = root_dir_block
            .checked_add(1)
            .ok_or(MkfsError::LayoutOverflow)?;

        write_superblock(&mut image, self.config);
        write_root_inode(&mut image, root_dir_block);
        write_root_dir_block(&mut image, root_dir_block);

        for (index, file) in self.files.iter().enumerate() {
            let inum = ROOT_INO + 1 + index as u32;
            let file_blocks = blocks_for_len(file.content.len())?;
            let layout = FileBlockLayout::new(next_data_block, file_blocks)?;

            write_file_inode(&mut image, inum, file.content.len() as u32, layout);
            write_file_data(&mut image, layout, &file.content);
            write_dirent(
                block_mut(&mut image, root_dir_block),
                (2 + index) * DIRENT_SIZE,
                inum,
                &file.name,
            );

            next_data_block = next_data_block
                .checked_add(layout.allocated_blocks)
                .ok_or(MkfsError::LayoutOverflow)?;
        }

        mark_used_blocks(&mut image, self.config, next_data_block);

        Ok(image)
    }
}

fn validate_files(config: MkfsConfig, files: &[FileEntry]) -> Result<(), MkfsError> {
    if files.len() + 2 > ROOT_DIRENTS {
        return Err(MkfsError::RootDirectoryFull);
    }
    if ROOT_INO + files.len() as u32 >= config.ninodes {
        return Err(MkfsError::NoInodes);
    }

    let mut required_data_blocks = 1u32;
    for file in files {
        required_data_blocks = required_data_blocks
            .checked_add(allocated_blocks_for_len(file.content.len())?)
            .ok_or(MkfsError::LayoutOverflow)?;
    }

    let used_blocks = data_start(config)?
        .checked_add(required_data_blocks)
        .ok_or(MkfsError::LayoutOverflow)?;
    if used_blocks > config.size {
        return Err(MkfsError::ImageTooSmall);
    }

    Ok(())
}

fn validate_file_name(name: &str) -> Result<(), MkfsError> {
    if name.is_empty() || name.len() > DIRSIZ {
        return Err(MkfsError::FileNameTooLong);
    }
    Ok(())
}

fn blocks_for_len(len: usize) -> Result<u32, MkfsError> {
    let blocks = len.div_ceil(BLOCK_SIZE);
    if blocks > MAXFILE {
        return Err(MkfsError::FileTooLarge);
    }
    Ok(blocks as u32)
}

fn allocated_blocks_for_len(len: usize) -> Result<u32, MkfsError> {
    let blocks = blocks_for_len(len)?;
    if blocks > NDIRECT as u32 {
        blocks.checked_add(1).ok_or(MkfsError::LayoutOverflow)
    } else {
        Ok(blocks)
    }
}

fn validate_config(config: MkfsConfig) -> Result<(), MkfsError> {
    if config.size < 4 {
        return Err(MkfsError::ImageTooSmall);
    }
    if config.ninodes <= ROOT_INO {
        return Err(MkfsError::NoInodes);
    }
    if used_blocks_for_empty_root(config)? > config.size {
        return Err(MkfsError::ImageTooSmall);
    }

    Ok(())
}

fn used_blocks_for_empty_root(config: MkfsConfig) -> Result<u32, MkfsError> {
    let used_without_data = data_start(config)?;
    used_without_data
        .checked_add(1)
        .ok_or(MkfsError::LayoutOverflow)
}

fn inode_blocks(config: MkfsConfig) -> u32 {
    config.ninodes / IPB + 1
}

fn bitmap_blocks(config: MkfsConfig) -> u32 {
    config.size.div_ceil(BPB)
}

fn inode_block(inum: u32) -> u32 {
    inum / IPB + 2
}

fn bitmap_block(block_no: u32, ninodes: u32) -> u32 {
    block_no / BPB + ninodes / IPB + 3
}

fn data_start(config: MkfsConfig) -> Result<u32, MkfsError> {
    2u32.checked_add(inode_blocks(config))
        .ok_or(MkfsError::LayoutOverflow)?
        .checked_add(bitmap_blocks(config))
        .ok_or(MkfsError::LayoutOverflow)
}

fn write_superblock(image: &mut [u8], config: MkfsConfig) {
    let block = block_mut(image, 1);
    write_u32(block, 0, config.size);
    write_u32(block, 4, config.nblocks);
    write_u32(block, 8, config.ninodes);
    write_u32(block, 12, config.nlog);
}

fn write_root_inode(image: &mut [u8], root_dir_block: u32) {
    write_inode(
        image,
        ROOT_INO,
        DiskInode {
            type_: T_DIR,
            nlink: 1,
            size: BLOCK_SIZE as u32,
            addrs: direct_addrs(root_dir_block, 1),
        },
    );
}

struct DiskInode {
    type_: u16,
    nlink: u16,
    size: u32,
    addrs: [u32; NDIRECT + 3],
}

#[derive(Debug, Clone, Copy)]
struct FileBlockLayout {
    first_block: u32,
    data_blocks: u32,
    allocated_blocks: u32,
}

impl FileBlockLayout {
    fn new(first_block: u32, data_blocks: u32) -> Result<Self, MkfsError> {
        let allocated_blocks = if data_blocks > NDIRECT as u32 {
            data_blocks
                .checked_add(1)
                .ok_or(MkfsError::LayoutOverflow)?
        } else {
            data_blocks
        };

        Ok(Self {
            first_block,
            data_blocks,
            allocated_blocks,
        })
    }

    fn indirect_block(self) -> Option<u32> {
        (self.data_blocks > NDIRECT as u32).then_some(self.first_block + NDIRECT as u32)
    }

    fn indirect_data_start(self) -> Option<u32> {
        self.indirect_block().map(|block| block + 1)
    }

    fn physical_data_block(self, logical_block: u32) -> u32 {
        if logical_block < NDIRECT as u32 {
            self.first_block + logical_block
        } else {
            self.indirect_data_start().expect("indirect data block")
                + (logical_block - NDIRECT as u32)
        }
    }
}

fn write_file_inode(image: &mut [u8], inum: u32, size: u32, layout: FileBlockLayout) {
    write_inode(
        image,
        inum,
        DiskInode {
            type_: T_FILE,
            nlink: 1,
            size,
            addrs: file_addrs(layout),
        },
    );
}

fn write_inode(image: &mut [u8], inum: u32, inode: DiskInode) {
    let block = block_mut(image, inode_block(inum));
    let offset = (inum % IPB) as usize * DINODE_SIZE;

    write_u16(block, offset, inode.type_);
    write_u16(block, offset + 6, inode.nlink);
    write_u32(block, offset + 8, inode.size);
    for (index, addr) in inode.addrs.iter().enumerate() {
        write_u32(block, offset + 12 + index * 4, *addr);
    }
}

fn direct_addrs(first_data_block: u32, blocks: u32) -> [u32; NDIRECT + 3] {
    let mut addrs = [0u32; NDIRECT + 3];
    for index in 0..blocks as usize {
        addrs[index] = first_data_block + index as u32;
    }
    addrs
}

fn file_addrs(layout: FileBlockLayout) -> [u32; NDIRECT + 3] {
    let direct_blocks = layout.data_blocks.min(NDIRECT as u32);
    let mut addrs = direct_addrs(layout.first_block, direct_blocks);
    if let Some(indirect_block) = layout.indirect_block() {
        addrs[NDIRECT] = indirect_block;
    }
    addrs
}

fn write_root_dir_block(image: &mut [u8], root_dir_block: u32) {
    let block = block_mut(image, root_dir_block);

    write_dirent(block, 0, ROOT_INO, ".");
    write_dirent(block, DIRENT_SIZE, ROOT_INO, "..");
}

fn write_dirent(block: &mut [u8], offset: usize, inum: u32, name: &str) {
    write_u16(block, offset, inum as u16);

    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(DIRSIZ);
    block[offset + 2..offset + 2 + name_len].copy_from_slice(&name_bytes[..name_len]);
}

fn write_file_data(image: &mut [u8], layout: FileBlockLayout, content: &[u8]) {
    for (index, chunk) in content.chunks(BLOCK_SIZE).enumerate() {
        let block = block_mut(image, layout.physical_data_block(index as u32));
        block[..chunk.len()].copy_from_slice(chunk);
    }

    if let Some(indirect_block_no) = layout.indirect_block() {
        let indirect_block = block_mut(image, indirect_block_no);
        let indirect_data_start = layout.indirect_data_start().expect("indirect data start");
        let indirect_data_blocks = layout.data_blocks - NDIRECT as u32;
        for index in 0..indirect_data_blocks {
            write_u32(
                indirect_block,
                index as usize * size_of::<u32>(),
                indirect_data_start + index,
            );
        }
    }
}

fn mark_used_blocks(image: &mut [u8], config: MkfsConfig, used_blocks: u32) {
    for block_no in 0..used_blocks {
        let bitmap_block_no = bitmap_block(block_no, config.ninodes);
        let bitmap = block_mut(image, bitmap_block_no);
        let byte_index = ((block_no % BPB) / 8) as usize;
        let mask = 1u8 << (block_no % 8);

        bitmap[byte_index] |= mask;
    }
}

fn block_mut(image: &mut [u8], block_no: u32) -> &mut [u8] {
    let start = block_no as usize * BLOCK_SIZE;
    &mut image[start..start + BLOCK_SIZE]
}

fn write_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MkfsConfig {
        MkfsConfig {
            size: 64,
            nblocks: 54,
            ninodes: 16,
            nlog: 0,
        }
    }

    fn read_u16(src: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(src[offset..offset + 2].try_into().unwrap())
    }

    fn read_u32(src: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(src[offset..offset + 4].try_into().unwrap())
    }

    fn block(image: &[u8], block_no: u32) -> &[u8] {
        let start = block_no as usize * BLOCK_SIZE;
        &image[start..start + BLOCK_SIZE]
    }

    fn inode(image: &[u8], inum: u32) -> &[u8] {
        let inode_block = block(image, inode_block(inum));
        let offset = (inum % IPB) as usize * DINODE_SIZE;
        &inode_block[offset..offset + DINODE_SIZE]
    }

    fn dirent_name(dir: &[u8], entry_index: usize) -> &str {
        let offset = entry_index * DIRENT_SIZE + 2;
        let name = &dir[offset..offset + DIRSIZ];
        let len = name.iter().position(|byte| *byte == 0).unwrap_or(DIRSIZ);
        core::str::from_utf8(&name[..len]).unwrap()
    }

    fn bitmap_used(image: &[u8], config: MkfsConfig, block_no: u32) -> bool {
        let bitmap = block(image, bitmap_block(block_no, config.ninodes));
        let byte_index = ((block_no % BPB) / 8) as usize;
        let mask = 1u8 << (block_no % 8);

        bitmap[byte_index] & mask != 0
    }

    #[test]
    fn empty_image_has_requested_block_count() {
        let image = format_empty_image(test_config()).expect("format empty filesystem");

        assert_eq!(image.len(), test_config().size as usize * BLOCK_SIZE);
    }

    #[test]
    fn block_zero_is_left_unused() {
        let image = format_empty_image(test_config()).expect("format empty filesystem");

        assert_eq!(block(&image, 0), &[0u8; BLOCK_SIZE]);
    }

    #[test]
    fn superblock_is_written_to_block_one() {
        let config = test_config();
        let image = format_empty_image(config).expect("format empty filesystem");
        let superblock = block(&image, 1);

        assert_eq!(read_u32(superblock, 0), config.size);
        assert_eq!(read_u32(superblock, 4), config.nblocks);
        assert_eq!(read_u32(superblock, 8), config.ninodes);
        assert_eq!(read_u32(superblock, 12), config.nlog);
    }

    #[test]
    fn root_inode_is_directory_with_one_data_block() {
        let config = test_config();
        let image = format_empty_image(config).expect("format empty filesystem");
        let inode_block = block(&image, inode_block(ROOT_INO));
        let offset = (ROOT_INO % IPB) as usize * DINODE_SIZE;
        let root_dir_block = data_start(config).unwrap();

        assert_eq!(read_u16(inode_block, offset), T_DIR);
        assert_eq!(read_u16(inode_block, offset + 6), 1);
        assert_eq!(read_u32(inode_block, offset + 8), BLOCK_SIZE as u32);
        assert_eq!(read_u32(inode_block, offset + 12), root_dir_block);
    }

    #[test]
    fn root_directory_contains_dot_and_dotdot() {
        let config = test_config();
        let image = format_empty_image(config).expect("format empty filesystem");
        let root_dir = block(&image, data_start(config).unwrap());

        assert_eq!(read_u16(root_dir, 0), ROOT_INO as u16);
        assert_eq!(&root_dir[2..3], b".");
        assert_eq!(root_dir[3], 0);

        assert_eq!(read_u16(root_dir, DIRENT_SIZE), ROOT_INO as u16);
        assert_eq!(&root_dir[DIRENT_SIZE + 2..DIRENT_SIZE + 4], b"..");
        assert_eq!(root_dir[DIRENT_SIZE + 4], 0);
    }

    #[test]
    fn bitmap_marks_metadata_and_root_directory_as_used() {
        let config = test_config();
        let image = format_empty_image(config).expect("format empty filesystem");
        let used_blocks = used_blocks_for_empty_root(config).unwrap();
        let bitmap = block(&image, bitmap_block(0, config.ninodes));

        for block_no in 0..used_blocks {
            let byte_index = ((block_no % BPB) / 8) as usize;
            let mask = 1u8 << (block_no % 8);

            assert_ne!(
                bitmap[byte_index] & mask,
                0,
                "block {block_no} should be used"
            );
        }

        let first_free = used_blocks;
        let byte_index = ((first_free % BPB) / 8) as usize;
        let mask = 1u8 << (first_free % 8);
        assert_eq!(bitmap[byte_index] & mask, 0);
    }

    #[test]
    fn image_too_small_is_rejected() {
        let mut config = test_config();
        config.size = data_start(config).unwrap();

        assert_eq!(
            format_empty_image(config).map(|_| ()),
            Err(MkfsError::ImageTooSmall)
        );
    }

    #[test]
    fn config_without_root_inode_is_rejected() {
        let mut config = test_config();
        config.ninodes = ROOT_INO;

        assert_eq!(
            format_empty_image(config).map(|_| ()),
            Err(MkfsError::NoInodes)
        );
    }

    #[test]
    fn adding_small_file_appends_root_directory_entry() {
        let config = test_config();
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", b"world").unwrap();

        let image = builder.build().expect("format filesystem");
        let root_dir = block(&image, data_start(config).unwrap());

        assert_eq!(read_u16(root_dir, 2 * DIRENT_SIZE), 2);
        assert_eq!(dirent_name(root_dir, 2), "hello");
    }

    #[test]
    fn file_inode_is_regular_file_with_content_size_and_direct_address() {
        let config = test_config();
        let content = b"small file";
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", content).unwrap();

        let image = builder.build().expect("format filesystem");
        let file_inode = inode(&image, 2);
        let file_data_block = data_start(config).unwrap() + 1;

        assert_eq!(read_u16(file_inode, 0), T_FILE);
        assert_eq!(read_u16(file_inode, 6), 1);
        assert_eq!(read_u32(file_inode, 8), content.len() as u32);
        assert_eq!(read_u32(file_inode, 12), file_data_block);
    }

    #[test]
    fn file_data_block_contains_content() {
        let config = test_config();
        let content = b"world";
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", content).unwrap();

        let image = builder.build().expect("format filesystem");
        let file_data = block(&image, data_start(config).unwrap() + 1);

        assert_eq!(&file_data[..content.len()], content);
        assert!(file_data[content.len()..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn bitmap_marks_file_data_block_as_used() {
        let config = test_config();
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", b"world").unwrap();

        let image = builder.build().expect("format filesystem");
        let file_data_block = data_start(config).unwrap() + 1;

        assert!(bitmap_used(&image, config, file_data_block));
        assert!(!bitmap_used(&image, config, file_data_block + 1));
    }

    #[test]
    fn multiple_files_use_distinct_inodes_and_data_blocks() {
        let config = test_config();
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("one", b"111").unwrap();
        builder.add_file("two", b"222").unwrap();

        let image = builder.build().expect("format filesystem");
        let root_dir = block(&image, data_start(config).unwrap());
        let first_inode = inode(&image, 2);
        let second_inode = inode(&image, 3);

        assert_eq!(read_u16(root_dir, 2 * DIRENT_SIZE), 2);
        assert_eq!(read_u16(root_dir, 3 * DIRENT_SIZE), 3);
        assert_eq!(dirent_name(root_dir, 2), "one");
        assert_eq!(dirent_name(root_dir, 3), "two");
        assert_ne!(read_u32(first_inode, 12), read_u32(second_inode, 12));
    }

    #[test]
    fn file_larger_than_one_block_is_split_across_direct_blocks() {
        let config = test_config();
        let mut content = vec![b'a'; BLOCK_SIZE];
        content.extend_from_slice(b"tail");
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("big", &content).unwrap();

        let image = builder.build().expect("format filesystem");
        let file_inode = inode(&image, 2);
        let first_block = data_start(config).unwrap() + 1;

        assert_eq!(read_u32(file_inode, 12), first_block);
        assert_eq!(read_u32(file_inode, 16), first_block + 1);
        assert_eq!(block(&image, first_block)[0], b'a');
        assert_eq!(&block(&image, first_block + 1)[..4], b"tail");
    }

    #[test]
    fn file_larger_than_direct_blocks_uses_single_indirect_block() {
        let config = test_config();
        let mut content = vec![b'd'; NDIRECT * BLOCK_SIZE];
        content.extend_from_slice(b"indirect");
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("indirect", &content).unwrap();

        let image = builder.build().expect("format filesystem");
        let file_inode = inode(&image, 2);
        let first_block = data_start(config).unwrap() + 1;
        let indirect_block_no = first_block + NDIRECT as u32;
        let first_indirect_data_block = indirect_block_no + 1;
        let indirect_block = block(&image, indirect_block_no);

        assert_eq!(read_u32(file_inode, 12), first_block);
        assert_eq!(
            read_u32(file_inode, 12 + NDIRECT * size_of::<u32>()),
            indirect_block_no
        );
        assert_eq!(read_u32(indirect_block, 0), first_indirect_data_block);
        assert_eq!(&block(&image, first_indirect_data_block)[..8], b"indirect");
    }

    #[test]
    fn bitmap_marks_single_indirect_block_and_indirect_data_as_used() {
        let config = test_config();
        let content = vec![0u8; NDIRECT * BLOCK_SIZE + 1];
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("indirect", content).unwrap();

        let image = builder.build().expect("format filesystem");
        let first_block = data_start(config).unwrap() + 1;
        let indirect_block_no = first_block + NDIRECT as u32;
        let indirect_data_block = indirect_block_no + 1;

        assert!(bitmap_used(&image, config, indirect_block_no));
        assert!(bitmap_used(&image, config, indirect_data_block));
        assert!(!bitmap_used(&image, config, indirect_data_block + 1));
    }

    #[test]
    fn file_larger_than_single_indirect_capacity_is_rejected() {
        let mut builder = MkfsBuilder::new(test_config());
        let content = vec![0u8; MAXFILE * BLOCK_SIZE + 1];

        assert_eq!(
            builder
                .add_file("too-big", content)
                .unwrap()
                .build()
                .map(|_| ()),
            Err(MkfsError::FileTooLarge)
        );
    }

    #[test]
    fn image_too_small_for_file_data_is_rejected() {
        let mut config = test_config();
        config.size = used_blocks_for_empty_root(config).unwrap();
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", b"world").unwrap();

        assert_eq!(builder.build().map(|_| ()), Err(MkfsError::ImageTooSmall));
    }

    #[test]
    fn inode_shortage_for_files_is_rejected() {
        let mut config = test_config();
        config.ninodes = 2;
        let mut builder = MkfsBuilder::new(config);
        builder.add_file("hello", b"world").unwrap();

        assert_eq!(builder.build().map(|_| ()), Err(MkfsError::NoInodes));
    }

    #[test]
    fn full_root_directory_is_rejected() {
        let mut config = test_config();
        config.ninodes = 64;
        let mut builder = MkfsBuilder::new(config);

        for index in 0..(ROOT_DIRENTS - 1) {
            builder.add_file(format!("f{index:02}"), b"x").unwrap();
        }

        assert_eq!(
            builder.build().map(|_| ()),
            Err(MkfsError::RootDirectoryFull)
        );
    }

    #[test]
    fn too_long_file_name_is_rejected() {
        let mut builder = MkfsBuilder::new(test_config());

        assert_eq!(
            builder.add_file("123456789012345", b"x").map(|_| ()),
            Err(MkfsError::FileNameTooLong)
        );
    }
}
