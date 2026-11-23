use std::fs::File;
use std::io::Write;

pub const FB_WIDTH: usize = 960;
pub const FB_HEIGHT: usize = 540;

#[derive(Debug, Clone)]
pub struct Framebuffer {
    pixels: Vec<u32>,
    dirty: bool,
    mode: i32, // 0 = text mode, 1 = framebuffer mode
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0xFF051009; FB_WIDTH * FB_HEIGHT], // Dark SiliconCraft CRT background
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

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, mut color: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if color & 0xFF000000 == 0 {
            color |= 0xFF000000;
        }
        let start_x = x.max(0) as usize;
        let end_x = x.saturating_add(w).clamp(0, FB_WIDTH as i32) as usize;
        let start_y = y.max(0) as usize;
        let end_y = y.saturating_add(h).clamp(0, FB_HEIGHT as i32) as usize;

        for py in start_y..end_y {
            let row_offset = py * FB_WIDTH;
            for px in start_x..end_x {
                self.pixels[row_offset + px] = color;
            }
        }
        self.dirty = true;
    }

    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        let (x0, y0, x1, y1) = (i64::from(x0), i64::from(y0), i64::from(x1), i64::from(y1));
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let x_major = dx >= dy;
        let (major_start, major_end, minor_start, minor_end, limit) = if x_major {
            (x0, x1, y0, y1, FB_WIDTH as i64 - 1)
        } else {
            (y0, y1, x0, x1, FB_HEIGHT as i64 - 1)
        };
        let major_direction = (major_end - major_start).signum();
        let minor_direction = (minor_end - minor_start).signum();
        let steps = dx.max(dy);
        let minor_steps = dx.min(dy);
        if steps == 0 {
            self.set_pixel(x0 as i32, y0 as i32, color);
            return;
        }
        // Skip invisible steps without changing the rounding of the rasterized line.
        let (first, last) = if major_direction > 0 {
            ((-major_start).max(0), (limit - major_start).min(steps))
        } else {
            ((major_start - limit).max(0), major_start.min(steps))
        };
        for step in first..=last {
            let minor_offset = ((step as u64 * minor_steps as u64 + (steps as u64 - 1) / 2)
                / steps as u64) as i64;
            let major = major_start + step * major_direction;
            let minor = minor_start + minor_offset * minor_direction;
            let (x, y) = if x_major { (major, minor) } else { (minor, major) };
            self.set_pixel(x as i32, y as i32, color);
        }
    }

    pub fn blit(&mut self, x: i32, y: i32, w: i32, h: i32, ram: &super::ram::Ram, ram_src: usize) {
        if w <= 0 || h <= 0 {
            return;
        }
        let (x, y, stride) = (i64::from(x), i64::from(y), i64::from(w));
        let ram_slice = ram.as_slice();
        for py in y.max(0)..(y + i64::from(h)).min(FB_HEIGHT as i64) {
            for px in x.max(0)..(x + stride).min(FB_WIDTH as i64) {
                let offset = (py - y) * stride + px - x;
                let Some(src) = usize::try_from(offset).ok().and_then(|offset| ram_src.checked_add(offset)) else {
                    continue;
                };
                if let Some(&color) = ram_slice.get(src) {
                    self.set_pixel(px as i32, py as i32, color as u32);
                }
            }
        }
    }

    pub fn render_ascii(&self) -> String {
        let mut out = String::new();
        // Downscale to 60 columns x 30 lines
        const SCALE_X: usize = 16;
        const SCALE_Y: usize = 18;
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
