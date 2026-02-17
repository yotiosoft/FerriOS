use volatile::Volatile;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;

/// VGA バッファの色を表す列挙型
#[allow(dead_code)]                             // enum Color に対する警告を抑制
#[derive(Debug, Clone, Copy, PartialEq, Eq)]    // コピー可能
#[repr(u8)]                                     // u8 で表現
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// 色情報を表す構造体
/// 前景色と背景色を表す
#[derive(Debug, Clone, Copy, PartialEq, Eq)]    // コピー可能
#[repr(transparent)]
struct ColorCode(u8);
impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// 画面上の文字とテキストバッファ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]                                      // フィールドの順番を保証
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}
const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// 画面書き出し
pub struct Writer {
    column_position: usize,                     // カーソルの横位置（最後の行）
    color_code: ColorCode,                      // 文字色
    buffer: &'static mut Buffer,                // テキストバッファ（参照は実行中常に有効）
}
impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\t' => {
                for _ in 0..4 {
                    self.write_byte(b' ');
                }
            }
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // 出力可能な ASCII byte または改行コード
                0x20..=0x7e | b'\n' | b'\t' => self.write_byte(byte),
                // 出力不可な文字 -> 特定の文字に置き換え
                _ => self.write_byte(0xfe),
            }
        }
    }
}

/// fmt::Write トレイトの実装
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

// static な Writer インスタンスを生成
lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    }); 
}

/// println!, print! マクロの実装
#[macro_export]
macro_rules! vga_print {
    ($($arg:tt)*) => ($crate::console::vga_buffer::_print(format_args!($($arg)*)));
}
#[macro_export]
macro_rules! vga_println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // ロック中の割り込みを防止
    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    })
}

// テスト
#[test_case]
fn test_println_simple() {
    vga_println!("test_println_simple output");
}
#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        vga_println!("test_println_many output");
    }
}
