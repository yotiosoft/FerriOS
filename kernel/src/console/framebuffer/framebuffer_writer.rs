// Refer to: https://github.com/rust-osdev/bootloader/blob/1d4166141c22c9aaab0136f20305a26093c9a981/common/src/framebuffer.rs

use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use noto_sans_mono_bitmap::{get_raster, FontWeight, RasterHeight};

/// フォントの設定
const FONT_WEIGHT: FontWeight = FontWeight::Bold;
const RASTER_HEIGHT: RasterHeight = RasterHeight::Size16;
const CHAR_WIDTH: usize = 8; // Size16 の Regular は幅 8px
const LINE_HEIGHT: usize = 16;

pub struct FrameBufferWriter {
    buffer: &'static mut [u8],
    info: FrameBufferInfo,
    pub x_pos: usize,
    pub y_pos: usize,
    pub color: [u8; 3],
    pub weight: FontWeight,
}

impl FrameBufferWriter {
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        Self {
            buffer,
            info,
            x_pos: 0,
            y_pos: 0,
            color: [255, 255, 0],
            weight: FONT_WEIGHT,
        }
    }

    /// バッファへの参照を返す（スクロール操作用）
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.buffer
    }

    fn newline(&mut self) {
        self.x_pos = 0;
        self.y_pos += LINE_HEIGHT;
    }

    pub fn clear(&mut self) {
        self.x_pos = 0;
        self.y_pos = 0;
        self.buffer.fill(0);
    }

    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            c => {
                if self.x_pos + CHAR_WIDTH > self.info.width {
                    self.newline();
                }
                // 画面の下端を超えそうになってもここでは何もしない。
                // スクロールの判定はこれを使うラッパー側で行う。
                
                self.draw_char(c);
                self.x_pos += CHAR_WIDTH;
            }
        }
    }

    fn draw_char(&mut self, c: char) {
        let char_raster = get_raster(c, self.weight, RASTER_HEIGHT)
            .unwrap_or_else(|| get_raster(' ', self.weight, RASTER_HEIGHT).unwrap());

        for (y, row) in char_raster.raster().iter().enumerate() {
            for (x, intensity) in row.iter().enumerate() {
                self.write_pixel(self.x_pos + x, self.y_pos + y, *intensity);
            }
        }
    }

    fn write_pixel(&mut self, x: usize, y: usize, intensity: u8) {
        let pixel_offset = y * self.info.stride + x;
        let r = (self.color[0] as u16 * intensity as u16 / 255) as u8;
        let g = (self.color[1] as u16 * intensity as u16 / 255) as u8;
        let b = (self.color[2] as u16 * intensity as u16 / 255) as u8;

        let color_bytes = match self.info.pixel_format {
            PixelFormat::Rgb => [r, g, b, 0],
            PixelFormat::Bgr => [b, g, r, 0],
            PixelFormat::U8 => [intensity, 0, 0, 0],
            _ => [r, g, b, 0],
        };

        let bytes_per_pixel = self.info.bytes_per_pixel;
        let byte_offset = pixel_offset * bytes_per_pixel;
        
        if byte_offset + bytes_per_pixel <= self.buffer.len() {
            self.buffer[byte_offset..(byte_offset + bytes_per_pixel)]
                .copy_from_slice(&color_bytes[..bytes_per_pixel]);
        }
    }
}
