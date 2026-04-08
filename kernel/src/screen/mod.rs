use core::fmt;

use font8x8::UnicodeFonts;
use boot::{FrameBufferInfo, PixelFormat};
pub(crate) use crate::flags::SCREEN_WRITER;

pub struct Writer {
    pub framebuffer: &'static mut [u8],
    pub info: FrameBufferInfo,
    pub x: usize,
    pub y: usize,
}

impl Writer {
    pub fn init(buffer: &'static mut [u8], info: FrameBufferInfo) {
        let mut writer = Writer::new(buffer, info);
        writer.clear(0, 0, 0);
        *SCREEN_WRITER.lock() = Some(writer);
    }

    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        Writer { framebuffer, info, x: 0, y: 0 }
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        let offset = (y * (self.info.stride as usize) + x) * (self.info.bytes_per_pixel as usize);
        match self.info.pixel_format {
            0 => {
                self.framebuffer[offset] = r;
                self.framebuffer[offset + 1] = g;
                self.framebuffer[offset + 2] = b;
            }
            1 => {
                self.framebuffer[offset] = b;
                self.framebuffer[offset + 1] = g;
                self.framebuffer[offset + 2] = r;
            }
            _ => {}
        }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        let width = self.info.width;
        let height = self.info.height;
        for y in 0..height {
            for x in 0..width {
                self.write_pixel((x as usize), (y as usize  ), r, g, b);
            }
        }
    }

    pub fn write_char(&mut self, x: usize, y: usize, c: char, r: u8, g: u8, b: u8) {
        let bitmap = font8x8::BASIC_FONTS.get(c).unwrap_or(font8x8::BASIC_FONTS.get(' ').unwrap());

        for (row, byte) in bitmap.iter().enumerate() {
            for col in 0..8 {
                if (byte >> col) & 1 == 1 {
                    self.write_pixel(x + col, y + row, r, g, b);
                }
            }
        }
    }

    pub fn write_string(&mut self, mut x: usize, y: usize, text: &str, r: u8, g: u8, b: u8) {
        for c in text.chars() {
            self.write_char(x, y, c, r, g, b);
            x += 8;
        }
    }

    pub fn write_string_at_cursor(&mut self, s: &str, r: u8, g: u8, b: u8) {
        for c in s.chars() {
            match c {
                '\x08' => {
                    if self.x >= 8 {
                        self.x -= 8;
                    } else if self.y >= 9 {
                        self.y -= 9;
                        self.x = ((self.info.width as usize) / 8) * 8 - 8;
                    }
                    for row in 0..8 {
                        for col in 0..8 {
                            self.write_pixel(self.x + col, self.y + row, 0, 0, 0);
                        }
                    }
                }
                '\n' => {
                    self.x = 0;
                    self.y += 9;

                    if self.y + 9 > (self.info.height as usize) {
                        self.scroll_up();
                        self.y -= 9;
                    }
                }
                _ => {
                    self.write_char(self.x, self.y, c, r, g, b);
                    self.x += 8;
                    if self.x + 8 > (self.info.width as usize) {
                        self.x = 0;
                        self.y += 9;

                        if self.y + 9 > (self.info.height as usize) {
                            self.scroll_up();
                            self.y -= 9;
                        }
                    }
                }
            }
        }
    }

    fn scroll_up(&mut self) {
        let row_height = 9usize;
        let width = self.info.width as usize;
        let height = self.info.height as usize;
        let bpp = self.info.bytes_per_pixel as usize;
        let stride = self.info.stride as usize;

        for y in row_height..height {
            for x in 0..width {
                let src = (y * stride + x) * bpp;
                let dst = ((y - row_height) * stride + x) * bpp;

                for i in 0..bpp {
                    self.framebuffer[dst + i] = self.framebuffer[src + i];
                }
            }
        }

        for y in (height - row_height)..height {
            for x in 0..width {
                self.write_pixel(x, y, 0, 0, 0);
            }
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string_at_cursor(s, 255, 255, 0);
        Ok(())
    }
}
