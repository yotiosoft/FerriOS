use super::*;

struct StackBuf {
    bytes: [u8; PRINT_FMT_BUF_LEN],
    len: usize,
}

impl StackBuf {
    const fn new() -> Self {
        Self {
            bytes: [0; PRINT_FMT_BUF_LEN],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for StackBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let new_len = self.len.checked_add(bytes.len()).ok_or(fmt::Error)?;
        if new_len > self.bytes.len() {
            return Err(fmt::Error);
        }

        self.bytes[self.len..new_len].copy_from_slice(bytes);
        self.len = new_len;
        Ok(())
    }
}

#[macro_export]
macro_rules! print_fmt {
    ($($arg:tt)*) => {{
        $crate::print_fmt(core::format_args!($($arg)*))
    }};
}

pub fn print_fmt(args: fmt::Arguments<'_>) -> SysRet {
    let mut buf = StackBuf::new();
    if buf.write_fmt(args).is_err() {
        return RET_ERROR;
    }
    print_str(buf.as_str())
}
