use std::fs;
use std::path::PathBuf;

const USER_CODE_START: u64 = 0x0000_1000_0000_0000;
const SYS_PRINT_NUM: u32 = 0;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let init_program = build_init_program();
    let init_elf = build_elf64_executable(USER_CODE_START, &init_program);

    fs::write(out_dir.join("init.elf"), init_elf).expect("failed to write init.elf");
}

fn build_init_program() -> Vec<u8> {
    let mut code = Vec::new();

    // mov rax, SYS_PRINT_NUM
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]);
    code.extend_from_slice(&SYS_PRINT_NUM.to_le_bytes());

    // mov rdi, 123
    code.extend_from_slice(&[0x48, 0xC7, 0xC7, 0x7B, 0x00, 0x00, 0x00]);

    // syscall
    code.extend_from_slice(&[0x0F, 0x05]);

    // jmp back to the syscall setup sequence
    code.extend_from_slice(&[0xEB, 0xEF]);

    code
}

fn build_elf64_executable(entry: u64, code: &[u8]) -> Vec<u8> {
    const ELF_HEADER_SIZE: u16 = 64;
    const PROGRAM_HEADER_SIZE: u16 = 56;
    const SEGMENT_OFFSET: usize = 0x1000;
    const ELF_TYPE_EXEC: u16 = 2;
    const ELF_MACHINE_X86_64: u16 = 0x3E;
    const ELF_VERSION_CURRENT: u32 = 1;
    const ELF_CLASS_64: u8 = 2;
    const ELF_DATA_LE: u8 = 1;
    const ELF_OSABI_SYSV: u8 = 0;
    const ELF_PT_LOAD: u32 = 1;
    const ELF_PF_R: u32 = 0x4;
    const ELF_PF_X: u32 = 0x1;

    let mut elf = vec![0u8; SEGMENT_OFFSET];

    elf[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    elf[4] = ELF_CLASS_64;
    elf[5] = ELF_DATA_LE;
    elf[6] = 1;
    elf[7] = ELF_OSABI_SYSV;

    elf[16..18].copy_from_slice(&ELF_TYPE_EXEC.to_le_bytes());
    elf[18..20].copy_from_slice(&ELF_MACHINE_X86_64.to_le_bytes());
    elf[20..24].copy_from_slice(&ELF_VERSION_CURRENT.to_le_bytes());
    elf[24..32].copy_from_slice(&entry.to_le_bytes());
    elf[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    elf[52..54].copy_from_slice(&ELF_HEADER_SIZE.to_le_bytes());
    elf[54..56].copy_from_slice(&PROGRAM_HEADER_SIZE.to_le_bytes());
    elf[56..58].copy_from_slice(&1u16.to_le_bytes());

    let phoff = ELF_HEADER_SIZE as usize;
    elf[phoff..phoff + 4].copy_from_slice(&ELF_PT_LOAD.to_le_bytes());
    elf[phoff + 4..phoff + 8].copy_from_slice(&(ELF_PF_R | ELF_PF_X).to_le_bytes());
    elf[phoff + 8..phoff + 16].copy_from_slice(&(SEGMENT_OFFSET as u64).to_le_bytes());
    elf[phoff + 16..phoff + 24].copy_from_slice(&USER_CODE_START.to_le_bytes());
    elf[phoff + 24..phoff + 32].copy_from_slice(&USER_CODE_START.to_le_bytes());
    elf[phoff + 32..phoff + 40].copy_from_slice(&(code.len() as u64).to_le_bytes());
    elf[phoff + 40..phoff + 48].copy_from_slice(&(code.len() as u64).to_le_bytes());
    elf[phoff + 48..phoff + 56].copy_from_slice(&(0x1000u64).to_le_bytes());

    elf.extend_from_slice(code);
    elf
}
