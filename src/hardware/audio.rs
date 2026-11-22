use std::fs::File;
use std::io::Write;

pub const AUDIO_SAMPLE_RATE: u32 = 8000;
pub const AUDIO_BUFFER_CAPACITY: usize = 65536;

#[derive(Debug, Clone)]
pub struct AudioSubsystem {
    buffer: Vec<u8>,
    head: usize,
    tail: usize,
    count: usize,
    triggered: bool,
    recorded_samples: Vec<u8>,
}

impl AudioSubsystem {
    pub fn new() -> Self {
        Self {
            buffer: vec![128; AUDIO_BUFFER_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            triggered: false,
            recorded_samples: Vec::new(),
        }
    }

    pub fn push_sample(&mut self, sample: u8) {
        if self.count >= self.buffer.len() {
            let new_len = self.buffer.len() * 2;
            let mut new_buf = vec![128; new_len];
            for i in 0..self.count {
                new_buf[i] = self.buffer[(self.head + i) % self.buffer.len()];
            }
            self.buffer = new_buf;
            self.head = 0;
            self.tail = self.count;
        }
        self.buffer[self.tail] = sample;
        self.tail = (self.tail + 1) % self.buffer.len();
        self.count += 1;
        self.recorded_samples.push(sample);
    }

    pub fn write_buffer(&mut self, ram: &super::ram::Ram, ram_src: usize, count: usize) {
        let ram_slice = ram.as_slice();
        for i in 0..count {
            let addr = ram_src + i;
            if addr < ram_slice.len() {
                self.push_sample((ram_slice[addr] & 0xFF) as u8);
            }
        }
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
        self.triggered = false;
    }

    pub fn stat(&self) -> usize {
        self.count
    }

    pub fn trigger_play(&mut self) {
        self.triggered = true;
    }

    pub fn generate_tone(&mut self, freq: i32, duration_ms: i32) {
        if duration_ms <= 0 {
            return;
        }
        let num_samples = ((AUDIO_SAMPLE_RATE as u64 * duration_ms as u64) / 1000) as usize;
        if freq <= 0 {
            // Silence / pause: 128 is center level in 8-bit unsigned PCM
            for _ in 0..num_samples {
                self.push_sample(128);
            }
        } else {
            let period = (AUDIO_SAMPLE_RATE as i32 / freq).max(1) as usize;
            let half = (period / 2).max(1);
            for i in 0..num_samples {
                let sample = if (i % period) < half { 200 } else { 50 };
                self.push_sample(sample);
            }
        }
        self.triggered = true;
    }

    pub fn poll_samples(&mut self) -> Option<Vec<u8>> {
        if !self.triggered && self.count < 512 {
            return None;
        }
        if self.count == 0 {
            self.triggered = false;
            return None;
        }
        let mut chunk = vec![0; self.count];
        for i in 0..self.count {
            chunk[i] = self.buffer[self.head];
            self.head = (self.head + 1) % self.buffer.len();
        }
        self.count = 0;
        self.triggered = false;
        Some(chunk)
    }

    pub fn recorded_samples(&self) -> &[u8] {
        &self.recorded_samples
    }

    pub fn save_wav(path: &str, samples: &[u8]) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        let num_samples = samples.len() as u32;
        let subchunk2_size = num_samples;
        let chunk_size = 36 + subchunk2_size;

        // RIFF header
        file.write_all(b"RIFF")?;
        file.write_all(&chunk_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;

        // "fmt " subchunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?; // Subchunk1Size = 16 for PCM
        file.write_all(&1u16.to_le_bytes())?;  // AudioFormat = 1 (PCM)
        file.write_all(&1u16.to_le_bytes())?;  // NumChannels = 1 (Mono)
        file.write_all(&AUDIO_SAMPLE_RATE.to_le_bytes())?; // SampleRate = 8000
        file.write_all(&AUDIO_SAMPLE_RATE.to_le_bytes())?; // ByteRate = 8000 * 1 * 1
        file.write_all(&1u16.to_le_bytes())?;  // BlockAlign = 1
        file.write_all(&8u16.to_le_bytes())?;   // BitsPerSample = 8

        // "data" subchunk
        file.write_all(b"data")?;
        file.write_all(&subchunk2_size.to_le_bytes())?;
        file.write_all(samples)?;

        Ok(())
    }
}
