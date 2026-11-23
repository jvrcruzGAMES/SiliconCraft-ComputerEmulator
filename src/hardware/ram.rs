#[derive(Debug, Clone)]
pub struct Ram {
    data: Vec<i32>,
}

impl Ram {
    pub const RAM_STICK_BYTES: usize = 2 * 1024 * 1024;
    pub const RAM_STICK_WORDS: usize = Self::RAM_STICK_BYTES / 4;
    pub const NUM_RAM_STICKS: usize = 8;
    pub const FOUR_STICKS_BYTES: usize = 4 * Self::RAM_STICK_BYTES;
    pub const FOUR_STICKS_WORDS: usize = 4 * Self::RAM_STICK_WORDS;
    pub const DEFAULT_SIZE: usize = Self::NUM_RAM_STICKS * Self::RAM_STICK_WORDS;

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
