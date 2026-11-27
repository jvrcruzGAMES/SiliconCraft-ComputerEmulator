use super::framebuffer::{FB_HEIGHT, FB_WIDTH};
use super::{Framebuffer, Ram};
use std::collections::VecDeque;

const VRAM_BYTES: usize = 2 * 1024 * 1024;
const COMPUTE_UNITS: usize = 4;
const QUEUE_CAPACITY: usize = 4096;

pub(crate) enum Command {
    Clear(u32),
    Pixel(i32, i32, u32),
    Rect([i32; 4], u32),
    Line([i32; 4], u32),
    Circle(i32, i32, i32, u32, bool),
    Triangle([(i32, i32); 3], u32),
    RamBlit([i32; 4], Vec<u32>),
    VramBlit([i32; 4], usize),
    Swap,
}

pub(crate) struct Gpu {
    pub front: Framebuffer,
    back: Framebuffer,
    queue: VecDeque<Command>,
    queued_ram_pixels: usize,
    vram: Vec<i32>,
    pub handler: i32,
    commands: i32,
    frames: i32,
}

impl Gpu {
    pub fn new() -> Self {
        Self {
            front: Framebuffer::new(),
            back: Framebuffer::new(),
            queue: VecDeque::new(),
            queued_ram_pixels: 0,
            vram: vec![0; VRAM_BYTES / 4],
            handler: 1,
            commands: 0,
            frames: 0,
        }
    }

    pub fn reset(&mut self) {
        self.discard_commands();
        self.front.set_mode(0);
        self.front.clear(0xff051009);
        self.back.clear(0xff051009);
        self.commands = 0;
        self.frames = 0;
    }

    pub fn discard_commands(&mut self) {
        self.queue.clear();
        self.queued_ram_pixels = 0;
    }

    pub fn selected(&self) -> bool {
        self.handler != 1
    }

    pub fn status(&self, query: i32) -> i32 {
        match query {
            0 => 1,
            1 => i32::from(self.selected()),
            2 => self.handler,
            3 => VRAM_BYTES as i32,
            4 => (VRAM_BYTES / 1024) as i32,
            5 => self.commands,
            6 => COMPUTE_UNITS as i32,
            7 => i32::from(!self.queue.is_empty()),
            8 => self.frames,
            _ => 0,
        }
    }

    pub fn submit(&mut self, command: Command) {
        let pixels = match &command {
            Command::RamBlit(_, pixels) => pixels.len(),
            _ => 0,
        };
        if self.queue.len() >= QUEUE_CAPACITY
            || self.queued_ram_pixels + pixels > 4 * FB_WIDTH * FB_HEIGHT
        {
            self.flush();
        }
        self.queued_ram_pixels += pixels;
        self.queue.push_back(command);
    }

    pub fn advance(&mut self) {
        for _ in 0..256 + COMPUTE_UNITS * 64 {
            let Some(command) = self.queue.pop_front() else {
                break;
            };
            self.execute(command);
        }
    }

    pub fn flush(&mut self) {
        while let Some(command) = self.queue.pop_front() {
            self.execute(command);
        }
    }

    pub fn read_back(&mut self, x: i32, y: i32) -> u32 {
        self.flush();
        self.back.get_pixel(x, y)
    }

    pub fn upload(
        &mut self,
        offset: i32,
        source: i32,
        count: i32,
        ram: &Ram,
    ) -> Result<(), String> {
        if offset < 0 || source < 0 || count < 0 {
            return Err("GPUVRAM upload out of bounds".into());
        }
        if count == 0 {
            return Ok(());
        }
        let (offset, source, count) = (offset as usize, source as usize, count as usize);
        let input = ram
            .as_slice()
            .get(source..source + count)
            .ok_or("GPUVRAM RAM source out of bounds")?;
        let output = self
            .vram
            .get_mut(offset..offset + count)
            .ok_or("GPUVRAM destination out of bounds")?;
        output.copy_from_slice(input);
        Ok(())
    }

    pub fn blit(&mut self, bounds: [i32; 4], offset: i32) -> Result<(), String> {
        let [_, _, w, h] = bounds;
        if w <= 0 || h <= 0 {
            return Ok(());
        }
        if offset < 0 || i64::from(offset) + i64::from(w) * i64::from(h) > self.vram.len() as i64 {
            return Err("GPUBLIT VRAM source out of bounds".into());
        }
        self.submit(Command::VramBlit(bounds, offset as usize));
        Ok(())
    }

    pub fn blit_ram(&mut self, [x, y, w, h]: [i32; 4], source: i32, ram: &Ram) {
        // Snapshot only visible pixels. Later RAM writes must not change a queued blit.
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = x.saturating_add(w).min(FB_WIDTH as i32);
        let y1 = y.saturating_add(h).min(FB_HEIGHT as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let mut pixels = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize);
        for py in y0..y1 {
            for px in x0..x1 {
                let index = i64::from(source)
                    + (i64::from(py) - i64::from(y)) * i64::from(w)
                    + i64::from(px)
                    - i64::from(x);
                let pixel = usize::try_from(index)
                    .ok()
                    .and_then(|i| ram.as_slice().get(i));
                pixels.push(pixel.copied().unwrap_or(0) as u32);
            }
        }
        self.submit(Command::RamBlit([x0, y0, x1 - x0, y1 - y0], pixels));
    }

    fn execute(&mut self, command: Command) {
        match command {
            Command::Clear(color) => self.back.clear(color),
            Command::Pixel(x, y, color) => self.back.set_pixel(x, y, color),
            Command::Rect([x, y, w, h], color) => self.back.fill_rect(x, y, w, h, color),
            Command::Line([x0, y0, x1, y1], color) => self.back.draw_line(x0, y0, x1, y1, color),
            Command::Circle(x, y, r, color, filled) => {
                circle(&mut self.back, x, y, r, color, filled)
            }
            Command::Triangle(vertices, color) => triangle(&mut self.back, vertices, color),
            Command::RamBlit(bounds, pixels) => {
                blit(&mut self.back, bounds, |i, _| Some(pixels[i]))
            }
            Command::VramBlit(bounds, offset) => blit(&mut self.back, bounds, |i, dst| {
                blend(self.vram[offset + i] as u32, dst)
            }),
            Command::Swap => {
                self.front.copy_pixels_from(&self.back);
                self.front.set_mode(1);
                self.frames = self.frames.wrapping_add(1);
            }
        }
        self.commands = self.commands.wrapping_add(1);
    }
}

fn span(frame: &mut Framebuffer, y: i64, left: i64, right: i64, color: u32) {
    let left = left.max(0);
    let right = right.min(FB_WIDTH as i64 - 1);
    if left <= right {
        frame.fill_rect(left as i32, y as i32, (right - left + 1) as i32, 1, color);
    }
}

fn circle(frame: &mut Framebuffer, cx: i32, cy: i32, radius: i32, color: u32, filled: bool) {
    if radius < 0 {
        return;
    }
    let (cx, cy, r) = (i64::from(cx), i64::from(cy), i64::from(radius));
    let radius_sq = r * r;
    for y in (cy - r).max(0)..=(cy + r).min(FB_HEIGHT as i64 - 1) {
        let row = (y - cy).abs();
        let remaining = radius_sq - row * row;
        let floor = remaining.isqrt();
        let rounded = floor + i64::from(remaining - floor * floor > floor);
        // The midpoint circle reflects the first octant across both axes and the diagonal.
        // Recover the reflected interval by inverting the rounded ordinate at this row.
        let min_sq = (remaining - row).max(0);
        let min = min_sq.isqrt();
        let start = min + i64::from(min * min < min_sq);
        let end = (remaining + row - 1).max(0).isqrt().min(row);
        if rounded >= row {
            if filled {
                span(frame, y, cx - rounded, cx + rounded, color);
            } else {
                span(frame, y, cx - rounded, cx - rounded, color);
                span(frame, y, cx + rounded, cx + rounded, color);
            }
        }
        if start <= end {
            if filled {
                span(frame, y, cx - end, cx + end, color);
            } else {
                span(frame, y, cx - end, cx - start, color);
                span(frame, y, cx + start, cx + end, color);
            }
        }
    }
}

fn triangle(frame: &mut Framebuffer, mut vertices: [(i32, i32); 3], color: u32) {
    vertices.sort_by_key(|&(_, y)| y);
    let [(x0, y0), (x1, y1), (x2, y2)] = vertices.map(|(x, y)| (i64::from(x), i64::from(y)));
    if y0 == y2 {
        if (0..FB_HEIGHT as i64).contains(&y0) {
            span(frame, y0, x0.min(x1).min(x2), x0.max(x1).max(x2), color);
        }
        return;
    }
    let interpolate = |(xa, ya): (i64, i64), (xb, yb): (i64, i64), y: i64| {
        xa + ((i128::from(xb - xa) * i128::from(y - ya)) / i128::from((yb - ya).max(1))) as i64
    };
    for y in y0.max(0)..=y2.min(FB_HEIGHT as i64 - 1) {
        let a = interpolate((x0, y0), (x2, y2), y);
        let b = if y > y1 || y0 == y1 {
            interpolate((x1, y1), (x2, y2), y)
        } else {
            interpolate((x0, y0), (x1, y1), y)
        };
        span(frame, y, a.min(b), a.max(b), color);
    }
}

fn blit(
    frame: &mut Framebuffer,
    [x, y, w, h]: [i32; 4],
    pixel: impl Fn(usize, u32) -> Option<u32>,
) {
    let (x, y, w, h) = (i64::from(x), i64::from(y), i64::from(w), i64::from(h));
    for py in y.max(0)..(y + h).min(FB_HEIGHT as i64) {
        for px in x.max(0)..(x + w).min(FB_WIDTH as i64) {
            let index = ((py - y) * w + px - x) as usize;
            let (px, py) = (px as i32, py as i32);
            if let Some(color) = pixel(index, frame.get_pixel(px, py)) {
                frame.set_pixel(px, py, color);
            }
        }
    }
}

fn blend(src: u32, dst: u32) -> Option<u32> {
    let alpha = src >> 24;
    match alpha {
        0 => None,
        255 => Some(src),
        _ => {
            let channel = |shift| {
                (((src >> shift) & 255u32) * alpha + ((dst >> shift) & 255u32) * (255 - alpha))
                    / 255u32
            };
            Some(0xff000000u32 | (channel(16) << 16) | (channel(8) << 8) | channel(0))
        }
    }
}
