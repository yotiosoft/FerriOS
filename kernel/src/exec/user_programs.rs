pub struct UserProgram {
    pub path: &'static str,
    pub elf: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/user_programs.rs"));

pub fn lookup(path: &str) -> Option<&'static [u8]> {
    PROGRAMS.iter().find(|program| program.path == path).map(|program| program.elf)
}
