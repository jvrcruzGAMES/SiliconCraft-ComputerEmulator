mod gamepads;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};
use pixels::{Pixels, PixelsBuilder, ScalingMode, SurfaceTexture};
use pixels::wgpu::Backends;
use font8x8::UnicodeFonts;
use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use crate::hardware::audio::AUDIO_SAMPLE_RATE;

use crate::cpu::{VirtualCpu, State};
use crate::hardware::framebuffer::{FB_WIDTH, FB_HEIGHT};

pub const WINDOW_WIDTH: u32 = 960;
pub const WINDOW_HEIGHT: u32 = 540;
pub const VIRTUAL_WIDTH: u32 = FB_WIDTH as u32;
pub const VIRTUAL_HEIGHT: u32 = FB_HEIGHT as u32;

const COLOR_BG: (u8, u8, u8) = (5, 16, 9);         // 0xFF051009
const COLOR_TEXT: (u8, u8, u8) = (51, 255, 102);   // 0xFF33FF66 (Phosphor green)
const COLOR_CYAN: (u8, u8, u8) = (102, 221, 255);  // 0xFF66DDFF
const COLOR_RED: (u8, u8, u8) = (255, 85, 85);     // 0xFFFF5555
const COLOR_AMBER: (u8, u8, u8) = (255, 179, 0);   // 0xFFFFB300
const COLOR_DIM: (u8, u8, u8) = (68, 119, 85);     // 0xFF447755

pub struct EmulatorWindow {
    cpu: VirtualCpu,
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    cycles_per_frame: usize,
    scroll_offset: usize,
    large_font: bool,
    start_time: Instant,
    gamepads: Option<gamepads::Gamepads>,
    audio: Option<AudioStreamOwner>,
    error: Option<String>,
    held_keys: HashSet<KeyCode>,
    cursor: Option<(i32, i32)>,
}

impl EmulatorWindow {
    pub fn new(cpu: VirtualCpu) -> Self {
        Self {
            cpu,
            window: None,
            pixels: None,
            cycles_per_frame: 41_666, // ~2.5 MHz at 60 FPS
            scroll_offset: 0,
            large_font: false,
            start_time: Instant::now(),
            gamepads: None,
            audio: None,
            error: None,
            held_keys: HashSet::new(),
            cursor: None,
        }
    }

    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        match gamepads::Gamepads::new() {
            Ok(gamepads) => self.gamepads = Some(gamepads),
            Err(error) => eprintln!("SDL3 controller input unavailable: {error}"),
        }
        match open_audio() {
            Ok(stream) => self.audio = Some(stream),
            Err(error) => eprintln!("SDL3 audio playback unavailable: {error}"),
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(&mut self)?;
        self.error.map_or(Ok(()), |error| Err(error.into()))
    }

    fn render_screen(&self, frame: &mut [u8]) {
        // Clear background
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = COLOR_BG.0;
            pixel[1] = COLOR_BG.1;
            pixel[2] = COLOR_BG.2;
            pixel[3] = 255;
        }

        if self.cpu.framebuffer.mode() == 1 {
            // --- Framebuffer Mode (960x540 1:1 into virtual frame) ---
            for fy in 0..FB_HEIGHT {
                for fx in 0..FB_WIDTH {
                    let c = self.cpu.framebuffer.get_pixel(fx as i32, fy as i32);
                    let r = ((c >> 16) & 0xFF) as u8;
                    let g = ((c >> 8) & 0xFF) as u8;
                    let b = (c & 0xFF) as u8;
                    let a = ((c >> 24) & 0xFF) as u8;
                    let alpha = if a == 0 { 255 } else { a };

                    Self::set_pixel(frame, fx, fy, (r, g, b, alpha));
                }
            }
        } else {
            // --- Text Console Mode (CRT Monitor Style) ---
            self.render_text_console(frame);
        }

        // Apply subtle CRT scanline effect (alternate rows slightly dimmed)
        for y in (1..VIRTUAL_HEIGHT).step_by(2) {
            let row_start = (y * VIRTUAL_WIDTH * 4) as usize;
            let row_end = row_start + (VIRTUAL_WIDTH * 4) as usize;
            for p in frame[row_start..row_end].chunks_exact_mut(4) {
                p[0] = (p[0] as u16 * 230 / 256) as u8;
                p[1] = (p[1] as u16 * 230 / 256) as u8;
                p[2] = (p[2] as u16 * 230 / 256) as u8;
            }
        }
    }

    fn render_text_console(&self, frame: &mut [u8]) {
        let is_off = self.cpu.state == State::Off;
        if is_off {
            self.draw_string(frame, 12, 12, "[PERSONAL COMPUTER POWERED OFF]", COLOR_DIM, self.large_font);
            self.draw_string(frame, 12, 28, "Press F5 to power on and reset.", COLOR_DIM, self.large_font);
            return;
        }

        // Collect all lines to display
        let mut display_lines = Vec::new();
        for line in &self.cpu.terminal_output {
            display_lines.push(line.clone());
        }
        if !self.cpu.line_buffer.is_empty() {
            display_lines.push(self.cpu.line_buffer.clone());
        }

        let char_w = if self.large_font { 16 } else { 8 };
        let char_h = if self.large_font { 18 } else { 10 };
        let max_cols = ((VIRTUAL_WIDTH - 24) / char_w as u32) as usize;
        let max_rows = ((VIRTUAL_HEIGHT - 24) / char_h as u32) as usize;

        // Wrap lines if they exceed max_cols
        let mut wrapped_lines = Vec::new();
        for line in &display_lines {
            let mut start = 0;
            for (column, (index, _)) in line.char_indices().enumerate() {
                if column > 0 && column % max_cols == 0 {
                    wrapped_lines.push(line[start..index].to_string());
                    start = index;
                }
            }
            wrapped_lines.push(line[start..].to_string());
        }

        // Calculate view window based on scroll offset
        let total = wrapped_lines.len();
        let start = if total > max_rows {
            let max_scroll = total - max_rows;
            let eff_scroll = self.scroll_offset.min(max_scroll);
            max_scroll - eff_scroll
        } else {
            0
        };
        let end = (start + max_rows).min(total);

        let mut y = 10;
        for i in start..end {
            let line = &wrapped_lines[i];
            let color = if line.contains("EXCEPTION") || line.contains("ERROR") || line.contains("Error") {
                COLOR_RED
            } else if line.contains("BIOS") || line.contains("OK") || line.contains("Done") || line.contains("[OUT]") {
                COLOR_CYAN
            } else if line.contains("WARN") {
                COLOR_AMBER
            } else {
                COLOR_TEXT
            };

            self.draw_string(frame, 12, y, line, color, self.large_font);
            y += char_h;
        }

        // Blinking cursor at the end of output
        let blink_on = (self.start_time.elapsed().as_millis() / 500) % 2 == 0;
        if blink_on && self.cpu.state != State::Halted {
            let last_line_len = wrapped_lines.last().map(|s| s.len()).unwrap_or(0);
            let cursor_x = 12 + (last_line_len as i32 * char_w);
            let cursor_y = (y - char_h).max(10);
            if (cursor_x as u32) < VIRTUAL_WIDTH - 12 {
                Self::draw_rect(frame, cursor_x, cursor_y, char_w as u32, (char_h - 2) as u32, (COLOR_TEXT.0, COLOR_TEXT.1, COLOR_TEXT.2, 200));
            }
        }
    }

    fn draw_string(&self, frame: &mut [u8], x: i32, y: i32, text: &str, color: (u8, u8, u8), large: bool) {
        let step = if large { 16 } else { 8 };
        let mut cur_x = x;
        for ch in text.chars() {
            if cur_x + step > VIRTUAL_WIDTH as i32 {
                break;
            }
            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                for (row, byte) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if (byte >> col) & 1 == 1 {
                            if large {
                                let px = cur_x + (col as i32 * 2);
                                let py = y + (row as i32 * 2);
                                Self::set_pixel(frame, px as usize, py as usize, (color.0, color.1, color.2, 255));
                                Self::set_pixel(frame, (px + 1) as usize, py as usize, (color.0, color.1, color.2, 255));
                                Self::set_pixel(frame, px as usize, (py + 1) as usize, (color.0, color.1, color.2, 255));
                                Self::set_pixel(frame, (px + 1) as usize, (py + 1) as usize, (color.0, color.1, color.2, 255));
                            } else {
                                let px = cur_x + col as i32;
                                let py = y + row as i32;
                                Self::set_pixel(frame, px as usize, py as usize, (color.0, color.1, color.2, 255));
                            }
                        }
                    }
                }
            }
            cur_x += step;
        }
    }

    fn draw_rect(frame: &mut [u8], x: i32, y: i32, w: u32, h: u32, color: (u8, u8, u8, u8)) {
        for dy in 0..h {
            for dx in 0..w {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && (px as u32) < VIRTUAL_WIDTH && py >= 0 && (py as u32) < VIRTUAL_HEIGHT {
                    Self::set_pixel(frame, px as usize, py as usize, color);
                }
            }
        }
    }

    fn set_pixel(frame: &mut [u8], x: usize, y: usize, color: (u8, u8, u8, u8)) {
        if x < VIRTUAL_WIDTH as usize && y < VIRTUAL_HEIGHT as usize {
            let idx = (y * VIRTUAL_WIDTH as usize + x) * 4;
            if idx + 3 < frame.len() {
                frame[idx] = color.0;
                frame[idx + 1] = color.1;
                frame[idx + 2] = color.2;
                frame[idx + 3] = color.3;
            }
        }
    }

    fn update_title(&self) {
        if let Some(ref window) = self.window {
            let state_str = match &self.cpu.state {
                State::Running => "RUNNING",
                State::Paused => "PAUSED",
                State::Halted => "HALTED",
                State::Off => "OFF",
                State::Error(_) => "ERROR",
            };
            let mode_str = if self.cpu.framebuffer.mode() == 1 { "FRAMEBUFFER" } else { "TEXT" };
            let title = format!(
                "SiliconCraft Computer [Vulkan] - {} [{}] | PC:{:04} | A:{:<5} B:{:<5} C:{:<5} D:{:<5}",
                state_str, mode_str, self.cpu.pc, self.cpu.reg_a, self.cpu.reg_b, self.cpu.reg_c, self.cpu.reg_d
            );
            window.set_title(&title);
        }
    }
}

impl ApplicationHandler for EmulatorWindow {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let win_attrs = Window::default_attributes()
                .with_title("SiliconCraft Virtual Computer [Vulkan]")
                .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64))
                .with_min_inner_size(winit::dpi::LogicalSize::new(480.0, 270.0));

            let window = match event_loop.create_window(win_attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    self.error = Some(format!("Failed to create window: {e}"));
                    event_loop.exit();
                    return;
                }
            };

            let size = window.inner_size();
            let surface_texture = SurfaceTexture::new(size.width, size.height, window.clone());
            let mut pixels = match PixelsBuilder::new(VIRTUAL_WIDTH, VIRTUAL_HEIGHT, surface_texture)
                .wgpu_backend(Backends::VULKAN)
                .build()
            {
                Ok(p) => p,
                Err(e) => {
                    self.error = Some(format!("Failed to initialize Vulkan pixel buffer: {e}"));
                    event_loop.exit();
                    return;
                }
            };

            pixels.set_scaling_mode(ScalingMode::Fill);
            if let Err(error) = pixels.resize_surface(size.width, size.height) {
                self.error = Some(format!("Failed to scale the display: {error}"));
                event_loop.exit();
                return;
            }
            self.window = Some(window);
            self.pixels = Some(pixels);
            self.update_title();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut pixels) = self.pixels {
                    if size.width > 0 && size.height > 0 {
                        let _ = pixels.resize_surface(size.width, size.height);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as i32,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as i32,
                };
                if lines > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_add(lines as usize);
                } else if lines < 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub((-lines) as usize);
                }
            }
            WindowEvent::Focused(false) => self.held_keys.clear(),
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = self.pixels.as_ref()
                    .and_then(|pixels| pixels.window_pos_to_pixel((position.x as f32, position.y as f32)).ok())
                    .map(|(x, y)| (x as i32, y as i32));
                if let Some((x, y)) = self.cursor {
                    self.cpu.queue_mouse_move_interrupt(x, y);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                if let Some((x, y)) = self.cursor {
                    let button = match button {
                        MouseButton::Left => 0,
                        MouseButton::Right => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Back => 3,
                        MouseButton::Forward => 4,
                        MouseButton::Other(button) => i32::from(button),
                    };
                    self.cpu.queue_mouse_click_interrupt(x, y, button);
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { state, physical_key, logical_key, text, .. }, .. } => {
                if let PhysicalKey::Code(code) = physical_key {
                    match state {
                        ElementState::Pressed => self.held_keys.insert(code),
                        ElementState::Released => self.held_keys.remove(&code),
                    };
                }
                if state == ElementState::Released {
                    return;
                }
                match logical_key {
                    Key::Named(NamedKey::F5) => {
                        // Reset CPU and power on
                        self.cpu.power_on();
                        if let Some(audio) = &self.audio {
                            let _ = audio.clear();
                        }
                        self.scroll_offset = 0;
                    }
                    Key::Named(NamedKey::F2) => {
                        // Toggle font scale
                        self.large_font = !self.large_font;
                    }
                    Key::Named(NamedKey::F10) => {
                        // Toggle pause
                        if self.cpu.state == State::Running {
                            self.cpu.state = State::Paused;
                        } else if self.cpu.state == State::Paused {
                            self.cpu.state = State::Running;
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.cpu.queue_keyboard_interrupt(10);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.cpu.queue_keyboard_interrupt(8);
                    }
                    Key::Named(NamedKey::Tab) => {
                        self.cpu.queue_keyboard_interrupt(9);
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.cpu.queue_keyboard_interrupt(27);
                    }
                    Key::Named(NamedKey::PageUp) => {
                        self.scroll_offset = self.scroll_offset.saturating_add(5);
                    }
                    Key::Named(NamedKey::PageDown) => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(5);
                    }
                    _ => {
                        if let Some(t) = text {
                            for ch in t.chars() {
                                if !ch.is_control() {
                                    self.cpu.queue_keyboard_interrupt(ch as i32);
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let mut masks = self.gamepads.as_mut().map_or([0; 4], gamepads::Gamepads::poll);
                masks[0] |= self.held_keys.iter().fold(0, |mask, &key| mask | key_mask(key));
                self.cpu.pad_states = masks;

                // Execute CPU cycles when running
                if self.cpu.state == State::Running {
                    self.cpu.tick(self.cycles_per_frame);
                }

                if let (Some(audio), Some(samples)) = (&self.audio, self.cpu.audio.poll_samples()) {
                    if let Err(error) = audio.put_data(&samples) {
                        eprintln!("SDL3 audio playback failed: {error}");
                        self.audio = None;
                    }
                }

                // Render screen into pixels frame buffer
                if let Some(mut pixels) = self.pixels.take() {
                    self.render_screen(pixels.frame_mut());
                    let res = pixels.render();
                    self.pixels = Some(pixels);
                    if let Err(e) = res {
                        self.error = Some(format!("Vulkan render error: {e}"));
                        event_loop.exit();
                        return;
                    }
                }

                self.update_title();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

pub fn run_emulator_window(cpu: VirtualCpu) -> Result<(), Box<dyn std::error::Error>> {
    let app = EmulatorWindow::new(cpu);
    app.run()
}


fn key_mask(key: KeyCode) -> i32 {
    use KeyCode::*;
    let bit = match key {
        ArrowRight | KeyD => 0,
        ArrowLeft | KeyA => 1,
        ArrowDown | KeyS => 2,
        ArrowUp | KeyW => 3,
        Enter | NumpadEnter => 4,
        ShiftLeft | ShiftRight => 5,
        Space | KeyZ => 6,
        KeyX => 7,
        KeyC | KeyJ => 8,
        KeyV | KeyK => 9,
        KeyQ | KeyU => 10,
        KeyE | KeyI => 11,
        _ => return 0,
    };
    1 << bit
}


fn open_audio() -> Result<AudioStreamOwner, Box<dyn std::error::Error>> {
    let sdl = sdl3::init()?;
    let audio = sdl.audio()?;
    let spec = AudioSpec::new(Some(AUDIO_SAMPLE_RATE as i32), Some(1), Some(AudioFormat::U8));
    let stream = audio.default_playback_device().open_device_stream(Some(&spec))?;
    stream.resume()?;
    Ok(stream)
}
