mod framebuffer_writer;

use framebuffer_writer::FrameBufferWriter;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo};
use core::fmt::{self, Write};
use spin::Mutex;

pub static WRITER: Mutex<Option<ScrollingWriter>> = Mutex::new(None);

pub struct ScrollingWriter {
    writer: FrameBufferWriter,
    info: FrameBufferInfo,
}

impl ScrollingWriter {
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let mut writer = FrameBufferWriter::new(buffer, info);
        writer.clear();
        Self { writer, info }
    }

    fn scroll(&mut self) {
        let stride = self.info.stride * self.info.bytes_per_pixel;
        let line_h = 16;
        let line_size = line_h * stride;
        let total_size = self.info.height * stride;
        
        let buffer = self.writer.buffer_mut();

        // メモリコピーで1行上にずらす
        buffer.copy_within(line_size..total_size, 0);
        
        // 最下行をクリア
        let last_start = (self.info.height - line_h) * stride;
        buffer[last_start..total_size].fill(0);

        // y座標を1行分戻す
        self.writer.y_pos -= line_h;
    }
}

impl fmt::Write for ScrollingWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            // 書き込む前に、画面から溢れるかどうかを判定
            // 次の行を描画する場所 (y_pos + 16) が高さを超えるならスクロール
            if self.writer.y_pos + 16 > self.info.height {
                self.scroll();
            }
            
            // 改行文字そのものや、右端到達による自動改行への対応
            if c == '\n' || self.writer.x_pos + 8 > self.info.width {
                if self.writer.y_pos + 32 > self.info.height {
                    self.scroll();
                }
            }
            
            self.writer.write_char(c);
        }
        Ok(())
    }
}

pub fn init(framebuffer: &'static mut FrameBuffer) {
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();
    *WRITER.lock() = Some(ScrollingWriter::new(buffer, info));
}

pub fn set_color(r: u8, g: u8, b: u8) {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.writer.color = [r, g, b];
    }
}

pub fn _print(args: fmt::Arguments) {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.write_fmt(args).unwrap();
    }
}
