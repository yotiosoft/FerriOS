pub struct UserProgram {
    pub path: &'static str,
    pub elf: &'static [u8],
}

pub static PROGRAMS: &[UserProgram] = &[UserProgram {
    path: "/init",
    elf: include_bytes!(concat!(env!("OUT_DIR"), "/init.elf")),
}];

pub fn lookup(path: &str) -> Option<&'static [u8]> {
    PROGRAMS.iter().find(|program| program.path == path).map(|program| program.elf)
}
