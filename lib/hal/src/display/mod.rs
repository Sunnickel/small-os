#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self { Self { r, g, b } }

    // Basics
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
    pub const YELLOW: Self = Self::new(255, 255, 0);
    pub const CYAN: Self = Self::new(0, 255, 255);
    pub const MAGENTA: Self = Self::new(255, 0, 255);

    // Grays
    pub const GRAY: Self = Self::new(128, 128, 128);
    pub const LIGHT_GRAY: Self = Self::new(192, 192, 192);
    pub const DARK_GRAY: Self = Self::new(64, 64, 64);

    // Dark variants
    pub const DARK_RED: Self = Self::new(139, 0, 0);
    pub const DARK_GREEN: Self = Self::new(0, 139, 0);
    pub const DARK_BLUE: Self = Self::new(0, 0, 139);

    // Light variants
    pub const LIGHT_RED: Self = Self::new(255, 102, 102);
    pub const LIGHT_GREEN: Self = Self::new(102, 255, 102);
    pub const LIGHT_BLUE: Self = Self::new(102, 102, 255);

    // Orange / Brown
    pub const ORANGE: Self = Self::new(255, 165, 0);
    pub const DARK_ORANGE: Self = Self::new(255, 140, 0);
    pub const BROWN: Self = Self::new(139, 69, 19);
    pub const TAN: Self = Self::new(210, 180, 140);

    // Pink / Purple
    pub const PINK: Self = Self::new(255, 182, 193);
    pub const HOT_PINK: Self = Self::new(255, 105, 180);
    pub const PURPLE: Self = Self::new(128, 0, 128);
    pub const VIOLET: Self = Self::new(238, 130, 238);
    pub const INDIGO: Self = Self::new(75, 0, 130);

    // Terminal classic 16 colors
    pub const BRIGHT_BLACK: Self = Self::new(85, 85, 85);
    pub const BRIGHT_RED: Self = Self::new(255, 85, 85);
    pub const BRIGHT_GREEN: Self = Self::new(85, 255, 85);
    pub const BRIGHT_YELLOW: Self = Self::new(255, 255, 85);
    pub const BRIGHT_BLUE: Self = Self::new(85, 85, 255);
    pub const BRIGHT_MAGENTA: Self = Self::new(255, 85, 255);
    pub const BRIGHT_CYAN: Self = Self::new(85, 255, 255);
    pub const BRIGHT_WHITE: Self = Self::new(255, 255, 255);

    // Misc
    pub const TRANSPARENT_BLACK: Self = Self::new(0, 0, 0); // placeholder, no alpha yet
    pub const CORNFLOWER_BLUE: Self = Self::new(100, 149, 237);
    pub const TEAL: Self = Self::new(0, 128, 128);
    pub const OLIVE: Self = Self::new(128, 128, 0);
    pub const MAROON: Self = Self::new(128, 0, 0);
    pub const NAVY: Self = Self::new(0, 0, 128);
    pub const LIME: Self = Self::new(0, 255, 0);
    pub const AQUA: Self = Self::new(0, 255, 255);
    pub const FUCHSIA: Self = Self::new(255, 0, 255);
    pub const SILVER: Self = Self::new(192, 192, 192);
    pub const GOLD: Self = Self::new(255, 215, 0);
    pub const SALMON: Self = Self::new(250, 128, 114);
    pub const CORAL: Self = Self::new(255, 127, 80);
    pub const TURQUOISE: Self = Self::new(64, 224, 208);
    pub const MINT: Self = Self::new(189, 252, 201);
    pub const LAVENDER: Self = Self::new(230, 230, 250);
    pub const BEIGE: Self = Self::new(245, 245, 220);
    pub const IVORY: Self = Self::new(255, 255, 240);
    pub const CRIMSON: Self = Self::new(220, 20, 60);
    pub const SCARLET: Self = Self::new(255, 36, 0);
}

impl Color {
    pub const fn to_rgb32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub const fn to_bgr32(self) -> u32 {
        ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
    }

    pub const fn from_rgb32(val: u32) -> Self {
        Self::new((val >> 16) as u8, (val >> 8) as u8, val as u8)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub trait Display {
    fn width(&self) -> u32;
    fn height(&self) -> u32;

    fn draw_pixel(&mut self, x: u32, y: u32, color: Color);
    fn fill_rect(&mut self, rect: Rect, color: Color);
    fn flush(&mut self);
    fn clear(&mut self, color: Color) {
        self.fill_rect(Rect { x: 0, y: 0, width: self.width(), height: self.height() }, color);
    }
}

pub trait TextDisplay: Display {
    fn draw_char(&mut self, x: u32, y: u32, c: char, color: Color);
    fn draw_str(&mut self, x: u32, y: u32, s: &str, color: Color) {
        let mut cx = x;
        for c in s.chars() {
            self.draw_char(cx, y, c, color);
            cx += 8;
        }
    }
}

pub trait FramebufferDisplay: Display {
    fn framebuffer(&mut self) -> &mut [u32];
    fn stride(&self) -> usize;
}