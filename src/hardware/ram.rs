#[derive(Debug, Clone)]
pub struct Ram {
    data: Vec<i32>,
}

impl Ram {
    /// Capacity of a single RAM stick in bytes: 1024 KB = 1,048,576 bytes
    pub const RAM_STICK_BYTES: usize = 1024 * 1024;
    /// Capacity of a single RAM stick in 32-bit words: 262,144 words
    pub const RAM_STICK_WORDS: usize = Self::RAM_STICK_BYTES / 4;
    /// Standard full complement: 4 sticks of RAM
    pub const NUM_RAM_STICKS: usize = 4;
    /// Full 4-stick capacity in bytes: 4096 KB (4 MB = 4,194,304 bytes)
    pub const FOUR_STICKS_BYTES: usize = Self::NUM_RAM_STICKS * Self::RAM_STICK_BYTES;
    /// Full 4-stick capacity in 32-bit words: 1,048,576 words
    pub const FOUR_STICKS_WORDS: usize = Self::NUM_RAM_STICKS * Self::RAM_STICK_WORDS;

    /// Default RAM capacity in emulator: 4 sticks of RAM (4096 KB / 1,048,576 words)
    pub const DEFAULT_SIZE: usize = Self::FOUR_STICKS_WORDS;

    pub fn new(capacity_words: usize) -> Self {
        Self {
            data: vec![0; capacity_words],
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn capacity_bytes(&self) -> usize {
        self.data.len() * 4
    }

    pub fn capacity_kb(&self) -> usize {
        self.capacity_bytes() / 1024
    }

    pub fn stick_count(&self) -> usize {
        self.data.len() / Self::RAM_STICK_WORDS
    }

    pub fn read(&self, addr: usize) -> Result<i32, String> {
        if addr < self.data.len() {
            Ok(self.data[addr])
        } else {
            Err(format!("RAM address {} out of bounds (size: {})", addr, self.data.len()))
        }
    }

    pub fn write(&mut self, addr: usize, val: i32) -> Result<(), String> {
        if addr < self.data.len() {
            self.data[addr] = val;
            Ok(())
        } else {
            Err(format!("RAM address {} out of bounds (size: {})", addr, self.data.len()))
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    pub fn as_slice(&self) -> &[i32] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [i32] {
        &mut self.data
    }

    pub fn dump(&self, start: usize, count: usize) -> String {
        let mut out = String::new();
        let end = (start + count).min(self.data.len());
        for i in (start..end).step_by(8) {
            out.push_str(&format!("[0x{:06X}] ", i));
            for j in 0..8 {
                if i + j < end {
                    out.push_str(&format!("{:10} ", self.data[i + j]));
                }
            }
            out.push('\n');
        }
        out
    }
}
