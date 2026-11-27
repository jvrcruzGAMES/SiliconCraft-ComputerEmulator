use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const MAX_DRIVES: usize = 4;
pub const DEFAULT_DRIVE_CAPACITY: usize = 2 * 1024 * 1024 * 1024;
const PAGE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveType {
    None,
    Flash,
    Nvme,
}

#[derive(Debug, Clone)]
enum DriveData {
    Memory(HashMap<usize, Box<[u8; PAGE_SIZE]>>),
    Image(Arc<Mutex<File>>),
}

impl DriveData {
    fn empty() -> Self {
        Self::Memory(HashMap::new())
    }

    fn read(&self, mut addr: usize, mut output: &mut [u8]) -> io::Result<()> {
        match self {
            Self::Image(file) => {
                let mut file = file
                    .lock()
                    .map_err(|_| io::Error::other("Drive image lock poisoned"))?;
                file.seek(SeekFrom::Start(addr as u64))?;
                while !output.is_empty() {
                    let count = file.read(output)?;
                    if count == 0 {
                        output.fill(0);
                        break;
                    }
                    output = &mut output[count..];
                }
            }
            Self::Memory(pages) => {
                while !output.is_empty() {
                    let offset = addr % PAGE_SIZE;
                    let count = (PAGE_SIZE - offset).min(output.len());
                    let (chunk, remaining) = output.split_at_mut(count);
                    if let Some(page) = pages.get(&(addr / PAGE_SIZE)) {
                        chunk.copy_from_slice(&page[offset..offset + count]);
                    } else {
                        chunk.fill(0);
                    }
                    addr += count;
                    output = remaining;
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, mut addr: usize, mut data: &[u8]) -> io::Result<()> {
        match self {
            Self::Image(file) => {
                let mut file = file
                    .lock()
                    .map_err(|_| io::Error::other("Drive image lock poisoned"))?;
                file.seek(SeekFrom::Start(addr as u64))?;
                file.write_all(data)?;
            }
            Self::Memory(pages) => {
                while !data.is_empty() {
                    let offset = addr % PAGE_SIZE;
                    let count = (PAGE_SIZE - offset).min(data.len());
                    let (chunk, remaining) = data.split_at(count);
                    let page = addr / PAGE_SIZE;
                    if chunk.iter().any(|&byte| byte != 0) {
                        pages
                            .entry(page)
                            .or_insert_with(|| Box::new([0; PAGE_SIZE]))
                            [offset..offset + count]
                            .copy_from_slice(chunk);
                    } else if let Some(page) = pages.get_mut(&page) {
                        page[offset..offset + count].fill(0);
                    }
                    addr += count;
                    data = remaining;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StorageSubsystem {
    drives: [DriveData; MAX_DRIVES],
    capacities: [usize; MAX_DRIVES],
    drive_types: [DriveType; MAX_DRIVES],
    drive_programs: [Vec<String>; MAX_DRIVES],
    selected_drive: usize,
}

impl StorageSubsystem {
    pub const DEFAULT_DRIVE_CAPACITY: usize = DEFAULT_DRIVE_CAPACITY;

    pub fn new() -> Self {
        const EMPTY_PROG: Vec<String> = Vec::new();
        Self {
            drives: std::array::from_fn(|_| DriveData::empty()),
            capacities: [
                DEFAULT_DRIVE_CAPACITY,
                DEFAULT_DRIVE_CAPACITY,
                DEFAULT_DRIVE_CAPACITY,
                0,
            ],
            // Onboard flash and the two storage expansion slots.
            drive_types: [
                DriveType::Flash,
                DriveType::Nvme,
                DriveType::Nvme,
                DriveType::None,
            ],
            drive_programs: [EMPTY_PROG; MAX_DRIVES],
            selected_drive: 0,
        }
    }

    pub fn has_nvme(&self) -> bool {
        self.drive_types.contains(&DriveType::Nvme)
    }

    pub fn drive_type(&self, drive: usize) -> DriveType {
        if drive < MAX_DRIVES {
            self.drive_types[drive]
        } else {
            DriveType::None
        }
    }

    pub fn set_drive_type(&mut self, drive: usize, dtype: DriveType) {
        if drive < MAX_DRIVES {
            self.drive_types[drive] = dtype;
        }
    }

    pub fn is_nvme_slot_1(&self) -> bool {
        self.drive_types[1] == DriveType::Nvme
    }

    pub fn attach_drive(&mut self, drive: usize, capacity_bytes: usize) {
        if drive < MAX_DRIVES {
            self.capacities[drive] = capacity_bytes;
            self.drives[drive] = DriveData::empty();
        }
    }

    /// Attach an existing or new sparse image without truncating its contents.
    pub fn attach_image(&mut self, drive: usize, path: &Path) -> io::Result<()> {
        let capacity = self.capacity(drive);
        if capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No drive installed in this slot",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        if file.metadata()?.len() > capacity as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Drive image exceeds the installed capacity",
            ));
        }
        self.drives[drive] = DriveData::Image(Arc::new(Mutex::new(file)));
        Ok(())
    }

    pub fn set_drive_program(&mut self, drive: usize, program: Vec<String>) {
        if drive < MAX_DRIVES {
            self.drive_programs[drive] = program;
        }
    }

    pub fn get_drive_program(&self, drive: usize) -> Option<&Vec<String>> {
        if drive < MAX_DRIVES && !self.drive_programs[drive].is_empty() {
            Some(&self.drive_programs[drive])
        } else {
            None
        }
    }

    pub(crate) fn read_program_binary(&self, drive: usize) -> Result<Option<Vec<u8>>, String> {
        let capacity = self.capacity(drive);
        if capacity < 4 {
            return Ok(None);
        }
        let mut header = [0; 4];
        self.drives[drive].read(0, &mut header).map_err(|error| error.to_string())?;
        if !crate::cpu::binary::has_magic(&header) {
            return Ok(None);
        }
        let mut bytes = vec![0; capacity.min(16 * 1024 * 1024)];
        self.drives[drive].read(0, &mut bytes).map_err(|error| error.to_string())?;
        Ok(Some(bytes))
    }

    pub(crate) fn read_program_text(&self, drive: usize) -> Option<Vec<String>> {
        let length = self.capacity(drive).min(4 * 1024 * 1024);
        if length == 0 {
            return None;
        }
        let mut bytes = vec![0; length];
        self.drives[drive].read(0, &mut bytes).ok()?;
        let length = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        let program = String::from_utf8_lossy(&bytes[..length]);
        let program = program.trim();
        (!program.is_empty()).then(|| program.lines().map(str::to_owned).collect())
    }

    pub fn selected_drive(&self) -> usize {
        self.selected_drive
    }

    pub fn select_drive(&mut self, drive: usize) -> Result<(), String> {
        if drive < MAX_DRIVES {
            self.selected_drive = drive;
            Ok(())
        } else {
            Err(format!(
                "Drive index {} out of bounds (max: {})",
                drive, MAX_DRIVES
            ))
        }
    }

    pub fn capacity(&self, drive: usize) -> usize {
        if drive < MAX_DRIVES {
            self.capacities[drive]
        } else {
            0
        }
    }

    pub fn drive_count(&self) -> usize {
        self.capacities.iter().filter(|&&c| c > 0).count()
    }

    pub fn read_byte(&self, drive: usize, addr: usize) -> Result<i32, String> {
        if drive >= MAX_DRIVES {
            return Err(format!("Drive index {} out of bounds", drive));
        }
        if addr >= self.capacities[drive] {
            return Err(format!(
                "Storage read address {} out of bounds on drive {}",
                addr, drive
            ));
        }
        let mut byte = [0];
        self.drives[drive]
            .read(addr, &mut byte)
            .map_err(|error| error.to_string())?;
        Ok(i32::from(byte[0]))
    }

    pub fn write_byte(&mut self, drive: usize, addr: usize, val: i32) -> Result<(), String> {
        if addr >= self.capacity(drive) {
            return Err(format!(
                "Storage write address {addr} out of bounds on drive {drive}"
            ));
        }
        self.drives[drive]
            .write(addr, &[val as u8])
            .map_err(|error| error.to_string())
    }

    pub fn read_word(&self, drive: usize, addr: usize) -> Result<i32, String> {
        self.read_byte(drive, addr)
    }

    pub fn write_word(&mut self, drive: usize, addr: usize, val: i32) -> Result<(), String> {
        self.write_byte(drive, addr, val)
    }

    pub fn block_read(
        &self,
        drive: usize,
        drive_src: usize,
        ram: &mut super::ram::Ram,
        ram_dest: usize,
        count: usize,
    ) -> Result<(), String> {
        if drive >= MAX_DRIVES {
            return Err(format!("Drive index {} out of bounds", drive));
        }
        let count = count.min(self.capacity(drive).saturating_sub(drive_src));
        let end = ram_dest.checked_add(count).ok_or("RAM address overflow")?;
        let words = ram
            .as_mut_slice()
            .get_mut(ram_dest..end)
            .ok_or("RAM block out of bounds")?;
        let mut bytes = [0; PAGE_SIZE];
        for (index, chunk) in words.chunks_mut(PAGE_SIZE).enumerate() {
            let address = drive_src + index * PAGE_SIZE;
            self.drives[drive]
                .read(address, &mut bytes[..chunk.len()])
                .map_err(|error| error.to_string())?;
            for (word, &byte) in chunk.iter_mut().zip(&bytes) {
                *word = i32::from(byte);
            }
        }
        Ok(())
    }

    pub fn block_write(
        &mut self,
        drive: usize,
        drive_dest: usize,
        ram: &super::ram::Ram,
        ram_src: usize,
        count: usize,
    ) -> Result<(), String> {
        if drive >= MAX_DRIVES || count > self.capacity(drive).saturating_sub(drive_dest) {
            return Err("Storage block write exceeds capacity".to_string());
        }
        let end = ram_src.checked_add(count).ok_or("RAM address overflow")?;
        let words = ram
            .as_slice()
            .get(ram_src..end)
            .ok_or("RAM block out of bounds")?;
        let mut bytes = [0; PAGE_SIZE];
        for (index, chunk) in words.chunks(PAGE_SIZE).enumerate() {
            for (&word, byte) in chunk.iter().zip(&mut bytes) {
                *byte = word as u8;
            }
            let address = drive_dest + index * PAGE_SIZE;
            self.drives[drive]
                .write(address, &bytes[..chunk.len()])
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
