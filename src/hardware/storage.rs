pub const MAX_DRIVES: usize = 4;
pub const DEFAULT_DRIVE_CAPACITY: usize = 1024 * 1024 * 1024; // 1024 MB (1,073,741,824 bytes)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveType {
    None,
    Flash,
    Nvme,
}

#[derive(Debug, Clone)]
pub struct StorageSubsystem {
    drives: [Vec<u8>; MAX_DRIVES],
    capacities: [usize; MAX_DRIVES],
    drive_types: [DriveType; MAX_DRIVES],
    drive_programs: [Vec<String>; MAX_DRIVES],
    selected_drive: usize,
    has_nvme: bool,
}

impl StorageSubsystem {
    pub const DEFAULT_DRIVE_CAPACITY: usize = DEFAULT_DRIVE_CAPACITY;

    pub fn new() -> Self {
        const EMPTY_VEC: Vec<u8> = Vec::new();
        const EMPTY_PROG: Vec<String> = Vec::new();
        Self {
            drives: [EMPTY_VEC; MAX_DRIVES],
            capacities: [DEFAULT_DRIVE_CAPACITY; MAX_DRIVES],
            // Drive 0 is Flash; Drive 1 is Extension Slot 1: NVMe Drive (1024 MB)
            drive_types: [DriveType::Flash, DriveType::Nvme, DriveType::None, DriveType::None],
            drive_programs: [EMPTY_PROG; MAX_DRIVES],
            selected_drive: 0,
            has_nvme: true,
        }
    }

    pub fn has_nvme(&self) -> bool {
        self.has_nvme || self.drive_types.iter().any(|&t| t == DriveType::Nvme)
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
            if dtype == DriveType::Nvme {
                self.has_nvme = true;
            }
        }
    }

    pub fn is_nvme_slot_1(&self) -> bool {
        self.drive_types[1] == DriveType::Nvme
    }

    pub fn attach_drive(&mut self, drive: usize, capacity_bytes: usize) {
        if drive < MAX_DRIVES {
            self.capacities[drive] = capacity_bytes;
            self.drives[drive] = vec![0; capacity_bytes];
        }
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

    pub fn selected_drive(&self) -> usize {
        self.selected_drive
    }

    pub fn select_drive(&mut self, drive: usize) -> Result<(), String> {
        if drive < MAX_DRIVES {
            self.selected_drive = drive;
            Ok(())
        } else {
            Err(format!("Drive index {} out of bounds (max: {})", drive, MAX_DRIVES))
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
            return Err(format!("Storage read address {} out of bounds on drive {}", addr, drive));
        }
        let buf = &self.drives[drive];
        if addr < buf.len() {
            Ok(buf[addr] as i32)
        } else {
            Ok(0)
        }
    }

    pub fn write_byte(&mut self, drive: usize, addr: usize, val: i32) -> Result<(), String> {
        if drive >= MAX_DRIVES {
            return Err(format!("Drive index {} out of bounds", drive));
        }
        if addr >= self.capacities[drive] {
            return Err(format!("Storage write address {} out of bounds on drive {}", addr, drive));
        }
        let buf = &mut self.drives[drive];
        if addr >= buf.len() {
            buf.resize(addr + 1, 0);
        }
        buf[addr] = (val & 0xFF) as u8;
        Ok(())
    }

    pub fn read_word(&self, drive: usize, addr: usize) -> Result<i32, String> {
        self.read_byte(drive, addr)
    }

    pub fn write_word(&mut self, drive: usize, addr: usize, val: i32) -> Result<(), String> {
        self.write_byte(drive, addr, val)
    }

    pub fn block_read(&self, drive: usize, drive_src: usize, ram: &mut super::ram::Ram, ram_dest: usize, count: usize) -> Result<(), String> {
        if drive >= MAX_DRIVES {
            return Err(format!("Drive index {} out of bounds", drive));
        }
        let cap = self.capacities[drive];
        let buf = &self.drives[drive];
        for i in 0..count {
            let cur_s = drive_src + i;
            let cur_r = ram_dest + i;
            if cur_s < cap {
                let val = if cur_s < buf.len() { buf[cur_s] as i32 } else { 0 };
                ram.write(cur_r, val)?;
            }
        }
        Ok(())
    }

    pub fn block_write(&mut self, drive: usize, drive_dest: usize, ram: &super::ram::Ram, ram_src: usize, count: usize) -> Result<(), String> {
        if drive >= MAX_DRIVES {
            return Err(format!("Drive index {} out of bounds", drive));
        }
        let cap = self.capacities[drive];
        let end = drive_dest + count;
        if end > cap {
            return Err(format!("Storage block write exceeds capacity: {}", end));
        }
        let buf = &mut self.drives[drive];
        if end > buf.len() {
            buf.resize(end, 0);
        }
        for i in 0..count {
            let cur_s = drive_dest + i;
            let cur_r = ram_src + i;
            let val = ram.read(cur_r)?;
            buf[cur_s] = (val & 0xFF) as u8;
        }
        Ok(())
    }
}
