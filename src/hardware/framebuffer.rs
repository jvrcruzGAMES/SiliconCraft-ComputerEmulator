use std::fs::File;
use std::io::Write;

pub const FB_WIDTH: usize = 240;
pub const FB_HEIGHT: usize = 135;

#[derive(Debug, Clone)]
pub struct Framebuffer {
    pixels: [u32; FB_WIDTH * FB_HEIGHT],
    dirty: bool,
    mode: i32, // 0 = text mode, 1 = framebuffer mode
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: [0xFF051009; FB_WIDTH * FB_HEIGHT], // Dark SiliconCraft CRT background
            dirty: false,
            mode: 0,
        }
    }

    pub fn mode(&self) -> i32 {
        self.mode
    }

    pub fn set_mode(&mut self, mode: i32) {
        self.mode = mode;
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, mut color: u32) {
        if (color & 0xFF000000) == 0 {
            color |= 0xFF000000;
        }
        if x >= 0 && (x as usize) < FB_WIDTH && y >= 0 && (y as usize) < FB_HEIGHT {
            let idx = (y as usize) * FB_WIDTH + (x as usize);
            self.pixels[idx] = color;
            self.dirty = true;
        }
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> u32 {
        if x >= 0 && (x as usize) < FB_WIDTH && y >= 0 && (y as usize) < FB_HEIGHT {
            let idx = (y as usize) * FB_WIDTH + (x as usize);
            self.pixels[idx]
        } else {
            0
        }
    }

    pub fn clear(&mut self, mut color: u32) {
        if (color & 0xFF000000) == 0 {
            color |= 0xFF000000;
        }
        self.pixels.fill(color);
        self.dirty = true;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let start_x = x.max(0) as usize;
        let end_x = ((x + w) as usize).min(FB_WIDTH);
        let start_y = y.max(0) as usize;
        let end_y = ((y + h) as usize).min(FB_HEIGHT);

        for py in start_y..end_y {
            let row_offset = py * FB_WIDTH;
            for px in start_x..end_x {
                self.pixels[row_offset + px] = color;
            }
        }
        self.dirty = true;
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.set_pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn blit(&mut self, x: i32, y: i32, w: i32, h: i32, ram: &super::ram::Ram, ram_src: usize) {
        if w <= 0 || h <= 0 {
            return;
        }
        let ram_slice = ram.as_slice();
        for row in 0..h {
            let py = y + row;
            if py < 0 || py as usize >= FB_HEIGHT {
                continue;
            }
            for col in 0..w {
                let px = x + col;
                if px < 0 || px as usize >= FB_WIDTH {
                    continue;
                }
                let src_idx = ram_src + (row as usize * w as usize) + col as usize;
                if src_idx < ram_slice.len() {
                    let mut color = ram_slice[src_idx] as u32;
                    if (color & 0xFF000000) == 0 {
                        color |= 0xFF000000;
                    }
                    self.pixels[(py as usize) * FB_WIDTH + px as usize] = color;
                }
            }
        }
        self.dirty = true;
    }

    pub fn render_ascii(&self) -> String {
        let mut out = String::new();
        // Downscale to 60 columns x 30 lines
        const SCALE_X: usize = 4;
        const SCALE_Y: usize = 4;
        let out_w = FB_WIDTH / SCALE_X;
        let out_h = FB_HEIGHT / SCALE_Y;

        out.push_str(&format!("┌{}┐\n", "─".repeat(out_w)));
        for oy in 0..out_h {
            out.push('│');
            for ox in 0..out_w {
                let py = oy * SCALE_Y;
                let px = ox * SCALE_X;
                let p = self.pixels[py * FB_WIDTH + px];
                let r = ((p >> 16) & 0xFF) as u32;
                let g = ((p >> 8) & 0xFF) as u32;
                let b = (p & 0xFF) as u32;
                let brightness = (r * 299 + g * 587 + b * 114) / 1000;
                let ch = match brightness {
                    0..=30 => ' ',
                    31..=70 => '.',
                    71..=120 => ':',
                    121..=180 => '*',
                    181..=230 => 'o',
                    _ => '#',
                };
                out.push(ch);
            }
            out.push_str("│\n");
        }
        out.push_str(&format!("└{}┘\n", "─".repeat(out_w)));
        out
    }

    pub fn save_ppm(&self, path: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        writeln!(file, "P3")?;
        writeln!(file, "{} {}", FB_WIDTH, FB_HEIGHT)?;
        writeln!(file, "255")?;
        for &pixel in &self.pixels {
            let r = (pixel >> 16) & 0xFF;
            let g = (pixel >> 8) & 0xFF;
            let b = pixel & 0xFF;
            writeln!(file, "{} {} {}", r, g, b)?;
        }
        Ok(())
    }
}
