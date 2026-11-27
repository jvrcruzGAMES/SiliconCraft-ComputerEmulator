pub mod n_compiler;
mod n2;
pub mod project;
pub mod instruction;
pub mod assembler;
pub mod peripheral;
pub mod network;
mod heap;

use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use instruction::{CompiledInstruction, Opcode, Operand, Register};
use assembler::Assembler;
use crate::hardware::gpu::{Command as GpuCommand, Gpu};
use crate::hardware::{AudioSubsystem, Framebuffer, Ram, StorageSubsystem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Off,
    Running,
    Paused,
    Halted,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct CpuContext {
    pub pc: usize,
    pub reg_a: i32,
    pub reg_b: i32,
    pub reg_c: i32,
    pub reg_d: i32,
    pub flag_z: bool,
    pub flag_s: bool,
    pub restore_regs: bool,
}

#[derive(Debug, Clone)]
pub struct InterruptEvent {
    pub vector: i32,
    pub reg_a: i32,
    pub reg_b: i32,
    pub reg_c: i32,
}

pub const VEC_DIV_ZERO: i32 = 0;
pub const VEC_ILLEGAL_OPCODE: i32 = 1;
pub const VEC_OUT_OF_BOUNDS: i32 = 2;
pub const VEC_STACK_FAULT: i32 = 3;
pub const VEC_GENERAL_EXCEPTION: i32 = 4;
pub const VEC_KEYBOARD: i32 = 10;
pub const VEC_MOUSE_MOVE: i32 = 11;
pub const VEC_MOUSE_CLICK: i32 = 12;
pub const VEC_TIMER: i32 = 13;
pub const VEC_RESET: i32 = 14;

const INSTRUCTIONS: &[&str] = &[
    "NOP", "MOV", "ADD", "SUB", "MUL", "DIV", "MOD", "INC", "DEC", "CMP",
    "AND", "OR", "XOR", "NOT", "SHL", "SHR", "JMP", "JZ", "JNZ", "JS",
    "JNS", "JL", "JLE", "JG", "JGE", "CALL", "RET", "PUSH", "POP", "PUSHA",
    "POPA", "IVEC", "IRET", "CLI", "STI", "INT", "OUT", "OUTH", "OUTC",
    "CLS", "SLEEP", "TIME", "RAND", "INPAD", "TIMER", "SETTIMER", "VBLANK",
    "SSIZE", "SREAD", "SWRITE", "SBLKR", "SBLKW", "SCOUNT", "SSEL", "SDRV",
    "SEXEC", "SBOOT", "FBMODE", "DMODE", "FBSET", "PIX", "FBGET", "FBCLR",
    "FCLR", "FBRECT", "RECT", "FBLINE", "LINE", "FBLIT", "FBSWAP", "FBWIDTH",
    "FBHEIGHT", "APUSH", "ABUFW", "ACLR", "ASTAT", "ATONE", "TONE", "APLAY",
    "PINFO", "PWRAP", "PCALL", "PCALLS", "PSTATUS", "PRESULT", "PLEN", "PRESULTS", "PSCAN",
    "NOPEN", "NSEND", "NRECV", "NCLOSE", "NHTTP", "NSTATUS", "NLEN", "NRESULT",
    "NWSOPEN", "NWSTEXT", "NTLS", "NHTTP3", "NLISTEN", "NACCEPT", "NRESPOND",
    "NRESPONDT", "NREADY", "NUTF8", "GALLOC",
    "INAXIS", "GFXSEL", "GFXSTAT", "GPUCLR", "GPURECT", "GPULINE",
    "GPUCIRC", "GPUTRI", "GPUVRAM", "GPUBLIT", "GPUSWAP", "GPUSYNC",
    "HLT", "ORG", "DB", "DW", "DL", "INCBIN"
];

pub struct VirtualCpu {
    pub reg_a: i32,
    pub reg_b: i32,
    pub reg_c: i32,
    pub reg_d: i32,
    pub pc: usize,
    pub flag_z: bool,
    pub flag_s: bool,
    pub cycles: u64,
    pub state: State,

    pub call_stack: Vec<usize>,
    pub data_stack: Vec<i32>,
    pub interrupt_stack: Vec<CpuContext>,
    pub interrupt_queue: VecDeque<InterruptEvent>,
    pub ivt: HashMap<i32, usize>,
    pub interrupts_enabled: bool,
    pub in_interrupt_handler: bool,

    pub timer_hz: i32,
    pub last_timer_nanos: u128,
    pub timer_interval_cycles: u64,
    pub last_timer_cycle: u64,
    pub sleep_ticks_remaining: i32,

    key_buffer: VecDeque<i32>,
    pub peripheral_bus: peripheral::PeripheralBus,
    pub network_port: network::NetworkPort,
    pub pad_states: [i32; 4],
    pad_axes: [[i32; 6]; 4],
    pub terminal_output: Vec<String>,
    pub line_buffer: String,
    utf8_bytes: Vec<u8>,

    pub ram: Ram,
    pub storage: StorageSubsystem,
    pub framebuffer: Framebuffer,
    gpu: Gpu,
    pub audio: AudioSubsystem,

    pub rom: Vec<String>,
    pub program: Vec<String>,
    pub compiled_program: Vec<CompiledInstruction>,
    pub labels: HashMap<String, usize>,
    pub data_labels: HashMap<String, usize>,
    pub supported_instructions: std::collections::HashSet<String>,
    start_time: Instant,
}


impl Default for VirtualCpu {
    fn default() -> Self {
        Self::new(crate::hardware::Ram::DEFAULT_SIZE)
    }
}

impl VirtualCpu {
    pub fn new(ram_words: usize) -> Self {
        Self {
            reg_a: 0,
            reg_b: 0,
            reg_c: 0,
            reg_d: 0,
            pc: 0,
            flag_z: false,
            flag_s: false,
            cycles: 0,
            state: State::Off,

            call_stack: Vec::new(),
            data_stack: Vec::new(),
            interrupt_stack: Vec::new(),
            interrupt_queue: VecDeque::new(),
            ivt: HashMap::new(),
            interrupts_enabled: true,
            in_interrupt_handler: false,

            timer_hz: 0,
            last_timer_nanos: 0,
            timer_interval_cycles: 0,
            last_timer_cycle: 0,
            sleep_ticks_remaining: 0,

            key_buffer: VecDeque::new(),
            peripheral_bus: peripheral::PeripheralBus::default(),
            network_port: network::NetworkPort::default(),
            pad_states: [0; 4],
            pad_axes: [[0; 6]; 4],
            terminal_output: Vec::new(),
            line_buffer: String::new(),
            utf8_bytes: Vec::new(),

            ram: Ram::new(ram_words),
            storage: StorageSubsystem::new(),
            framebuffer: Framebuffer::new(),
            gpu: Gpu::new(),
            audio: AudioSubsystem::new(),

            rom: Vec::new(),
            program: Vec::new(),
            compiled_program: Vec::new(),
            labels: HashMap::new(),
            data_labels: HashMap::new(),
            supported_instructions: INSTRUCTIONS.iter().map(|&name| name.to_string()).collect(),
            start_time: Instant::now(),
        }
    }

    pub fn load_program(&mut self, source_lines: &[String]) {
        self.rom = source_lines.to_vec();
        self.program = self.rom.clone();
        self.preprocess_labels();
        self.compile_bytecode();
        self.power_on();
    }

    pub fn burn_to_rom<T: AsRef<str>>(&mut self, source_lines: &[T]) {
        let lines: Vec<String> = source_lines.iter().map(|s| s.as_ref().to_string()).collect();
        self.load_program(&lines);
    }

    pub fn is_code_burned_to_rom(&self) -> bool {
        !self.rom.is_empty()
    }

    pub fn rom(&self) -> &[String] {
        &self.rom
    }

    pub fn has_nvme(&self) -> bool {
        self.storage.has_nvme()
    }

    pub fn extension_slot_1_is_nvme(&self) -> bool {
        self.storage.is_nvme_slot_1()
    }

    pub fn ram_stick_count(&self) -> usize {
        self.ram.stick_count()
    }

    pub fn ram_capacity_bytes(&self) -> usize {
        self.ram.capacity_bytes()
    }

    pub fn ram_capacity_kb(&self) -> usize {
        self.ram.capacity_kb()
    }

    pub fn enable_all_instructions(&mut self) {
        self.supported_instructions.extend(INSTRUCTIONS.iter().map(|&name| name.to_string()));
    }

    pub fn is_instruction_supported(&self, name: &str) -> bool {
        self.supported_instructions.contains(&name.to_uppercase())
    }

    pub fn load_program_lines<T: AsRef<str>>(&mut self, source_lines: &[T]) -> Result<(), String> {
        let lines: Vec<String> = source_lines.iter().map(|s| s.as_ref().to_string()).collect();
        self.load_program(&lines);
        Ok(())
    }

    pub fn execute_cycles(&mut self, max_cycles: usize) {
        self.tick(max_cycles);
    }

    pub fn read_reg(&self, r: Register) -> i32 {
        self.reg(r)
    }

    pub fn preprocess_labels(&mut self) {
        self.labels.clear();
        self.data_labels.clear();

        for (i, raw) in self.program.iter().enumerate() {
            let clean = Assembler::strip_comments(raw).trim().to_string();
            if clean.is_empty() {
                continue;
            }
            if let Some(colon) = Assembler::find_label_colon(&clean) {
                let lbl = clean[..colon].trim().to_lowercase();
                if !lbl.is_empty() {
                    self.labels.insert(lbl, i);
                }
            }
        }

        Assembler::load_static_data(
            &self.program,
            &mut self.ram,
            &mut self.labels,
            &mut self.data_labels,
            None,
        );
    }

    pub fn compile_bytecode(&mut self) {
        let mut compiled = Vec::with_capacity(self.program.len());
        for (i, raw_line) in self.program.iter().enumerate() {
            compiled.push(self.compile_line(i, raw_line));
        }
        self.compiled_program = compiled;
    }

    fn compile_line(&self, _line_num: usize, raw_line: &str) -> CompiledInstruction {
        let clean = Assembler::strip_comments(raw_line).trim().to_string();
        if clean.is_empty() || clean.ends_with(':') {
            return CompiledInstruction {
                opcode: Opcode::NOP,
                dest: Operand::None,
                src: Operand::None,
                target_pc: None,
                raw_line: raw_line.to_string(),
                raw_args: String::new(),
            };
        }

        let mut rest = clean.as_str();
        if let Some(colon) = Assembler::find_label_colon(&clean) {
            rest = clean[colon + 1..].trim();
            if rest.is_empty() {
                return CompiledInstruction {
                    opcode: Opcode::NOP,
                    dest: Operand::None,
                    src: Operand::None,
                    target_pc: None,
                    raw_line: raw_line.to_string(),
                    raw_args: String::new(),
                };
            }
        }

        let space_idx = rest.find(' ');
        let op_str = space_idx.map(|s| &rest[..s]).unwrap_or(rest).trim();
        let args = space_idx.map(|s| rest[s + 1..].trim()).unwrap_or("");
        let opcode = Opcode::from_str(op_str);

        let mut dest = Operand::None;
        let mut src = Operand::None;
        let mut target_pc = None;

        if opcode.is_jump() && !args.is_empty() {
            if let Ok(imm) = Assembler::parse_literal(args, &self.labels) {
                if imm >= 0 {
                    target_pc = Some(imm as usize);
                }
            }
        } else if !args.is_empty() {
            let parts = Assembler::split_args(args, 2);
            if !parts.is_empty() {
                dest = Assembler::parse_operand(&parts[0], &self.labels);
            }
            if parts.len() >= 2 {
                src = Assembler::parse_operand(&parts[1], &self.labels);
            }
        }

        CompiledInstruction {
            opcode,
            dest,
            src,
            target_pc,
            raw_line: raw_line.to_string(),
            raw_args: args.to_string(),
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.reg_a = 0;
        self.reg_b = 0;
        self.reg_c = 0;
        self.reg_d = 0;
        self.flag_z = false;
        self.flag_s = false;
        self.cycles = 0;
        self.call_stack.clear();
        self.data_stack.clear();
        self.interrupt_stack.clear();
        self.interrupt_queue.clear();
        self.interrupts_enabled = true;
        self.in_interrupt_handler = false;
        self.sleep_ticks_remaining = 0;
        self.timer_hz = 0;
        self.timer_interval_cycles = 0;
        self.pad_states = [0; 4];
        self.pad_axes = [[0; 6]; 4];
        self.key_buffer.clear();
        self.peripheral_bus.reset();
        self.network_port.reset();
        self.terminal_output.clear();
        self.line_buffer.clear();
        self.utf8_bytes.clear();
        self.framebuffer.set_mode(0);
        self.framebuffer.clear(0xFF051009);
        self.gpu.reset();
        let drive = (0..crate::hardware::storage::MAX_DRIVES)
            .find(|&drive| self.storage.capacity(drive) > 0).unwrap_or(0);
        let _ = self.storage.select_drive(drive);
        self.audio.clear();
        self.start_time = Instant::now();

        Assembler::load_static_data(
            &self.program,
            &mut self.ram,
            &mut self.labels,
            &mut self.data_labels,
            None,
        );

        if self.state != State::Off {
            self.state = State::Running;
        }
        self.queue_reset_interrupt();
    }

    pub fn power_on(&mut self) {
        self.reset();
        self.state = State::Running;
    }

    pub fn power_off(&mut self) {
        self.flush_line_buffer();
        self.state = State::Off;
        self.gpu.discard_commands();
        self.pad_states = [0; 4];
        self.pad_axes = [[0; 6]; 4];
        self.peripheral_bus.reset();
        self.network_port.reset();
        self.audio.clear();
        self.terminal_output.push("[POWER OFF]".to_string());
    }

    pub fn reg(&self, r: Register) -> i32 {
        match r {
            Register::A => self.reg_a,
            Register::B => self.reg_b,
            Register::C => self.reg_c,
            Register::D => self.reg_d,
            Register::PC => self.pc as i32,
            Register::FLAGS => (if self.flag_z { 1 } else { 0 }) | (if self.flag_s { 2 } else { 0 }),
        }
    }

    pub fn set_reg(&mut self, r: Register, val: i32) {
        match r {
            Register::A => self.reg_a = val,
            Register::B => self.reg_b = val,
            Register::C => self.reg_c = val,
            Register::D => self.reg_d = val,
            Register::PC => self.pc = val.max(0) as usize,
            Register::FLAGS => {
                self.flag_z = (val & 1) != 0;
                self.flag_s = (val & 2) != 0;
            }
        }
    }

    pub fn update_flags(&mut self, result: i32) {
        self.flag_z = result == 0;
        self.flag_s = result < 0;
    }

    pub fn queue_reset_interrupt(&mut self) {
        for event in &self.interrupt_queue {
            if event.vector == VEC_RESET {
                return;
            }
        }
        if self.interrupt_queue.len() < 64 {
            self.interrupt_queue.push_front(InterruptEvent {
                vector: VEC_RESET,
                reg_a: 0,
                reg_b: 0,
                reg_c: 0,
            });
        }
    }

    pub fn queue_timer_interrupt(&mut self) {
        for event in &self.interrupt_queue {
            if event.vector == VEC_TIMER {
                return;
            }
        }
        if self.interrupt_queue.len() < 64 {
            self.interrupt_queue.push_back(InterruptEvent {
                vector: VEC_TIMER,
                reg_a: 0,
                reg_b: 0,
                reg_c: 0,
            });
        }
    }

    pub fn queue_keyboard_interrupt(&mut self, key_char: i32) {
        if self.key_buffer.len() < 64 {
            self.key_buffer.push_back(key_char);
        }
        if self.interrupt_queue.len() < 64 {
            self.interrupt_queue.push_back(InterruptEvent {
                vector: VEC_KEYBOARD,
                reg_a: key_char,
                reg_b: 0,
                reg_c: 0,
            });
        }
    }

    pub fn queue_mouse_move_interrupt(&mut self, x: i32, y: i32) {
        if let Some(last) = self.interrupt_queue.back_mut() {
            if last.vector == VEC_MOUSE_MOVE {
                last.reg_a = x;
                last.reg_b = y;
                return;
            }
        }
        if self.interrupt_queue.len() < 64 {
            self.interrupt_queue.push_back(InterruptEvent {
                vector: VEC_MOUSE_MOVE,
                reg_a: x,
                reg_b: y,
                reg_c: 0,
            });
        }
    }

    pub fn queue_mouse_click_interrupt(&mut self, x: i32, y: i32, button: i32) {
        if self.interrupt_queue.len() < 64 {
            self.interrupt_queue.push_back(InterruptEvent {
                vector: VEC_MOUSE_CLICK,
                reg_a: x,
                reg_b: y,
                reg_c: button,
            });
        }
    }

    pub fn set_pad_state(&mut self, pad_idx: usize, mask: i32) {
        self.set_gamepad_state(pad_idx, mask, [0; 6]);
    }

    pub fn set_gamepad_state(&mut self, pad_idx: usize, buttons: i32, mut axes: [i32; 6]) {
        if pad_idx < self.pad_states.len() {
            for (axis, value) in axes.iter_mut().enumerate() {
                *value = (*value).clamp(if axis < 4 { -1000 } else { 0 }, 1000);
            }
            self.pad_states[pad_idx] = buttons & 0xffff;
            self.pad_axes[pad_idx] = axes;
        }
    }

    pub fn active_framebuffer(&self) -> &Framebuffer {
        if self.gpu.selected() { &self.gpu.front } else { &self.framebuffer }
    }

    pub fn set_timer_hz(&mut self, hz: i32) {
        self.timer_hz = hz.max(0);
        self.last_timer_nanos = self.start_time.elapsed().as_nanos();
        self.last_timer_cycle = self.cycles;
        self.timer_interval_cycles = if hz > 0 { (1000 / hz).max(1) as u64 } else { 0 };
    }

    pub fn check_timer_trigger(&mut self) {
        if self.timer_hz <= 0 {
            return;
        }
        let now = self.start_time.elapsed().as_nanos();
        let interval_nanos = 1_000_000_000u128 / (self.timer_hz as u128);
        if (now - self.last_timer_nanos >= interval_nanos)
            || (self.timer_interval_cycles > 0 && self.cycles - self.last_timer_cycle >= self.timer_interval_cycles)
        {
            self.last_timer_nanos = now;
            self.last_timer_cycle = self.cycles;
            self.queue_timer_interrupt();
        }
    }

    pub fn check_and_dispatch_interrupt(&mut self) {
        if !self.interrupts_enabled || self.in_interrupt_handler || self.interrupt_queue.is_empty() {
            return;
        }
        if let Some(event) = self.interrupt_queue.pop_front() {
            if let Some(&handler_pc) = self.ivt.get(&event.vector) {
                self.interrupt_stack.push(CpuContext {
                    pc: self.pc,
                    reg_a: self.reg_a,
                    reg_b: self.reg_b,
                    reg_c: self.reg_c,
                    reg_d: self.reg_d,
                    flag_z: self.flag_z,
                    flag_s: self.flag_s,
                    restore_regs: true,
                });
                self.in_interrupt_handler = true;
                self.reg_a = event.reg_a;
                self.reg_b = event.reg_b;
                self.reg_c = event.reg_c;
                self.pc = handler_pc;
                self.sleep_ticks_remaining = 0;
            }
        }
    }

    pub fn trigger_exception(&mut self, vector: i32, desc: &str, line_num: usize, code: &str) {
        if let Some(&handler_pc) = self.ivt.get(&vector) {
            self.interrupt_stack.push(CpuContext {
                pc: self.pc,
                reg_a: self.reg_a,
                reg_b: self.reg_b,
                reg_c: self.reg_c,
                reg_d: self.reg_d,
                flag_z: self.flag_z,
                flag_s: self.flag_s,
                restore_regs: false,
            });
            self.in_interrupt_handler = true;
            self.reg_a = vector;
            self.reg_b = line_num as i32;
            self.pc = handler_pc;
            return;
        }

        self.state = State::Error(format!(
            "Exception not handled: {} at line {}: '{}'",
            desc, line_num + 1, code
        ));
        self.flush_line_buffer();
        self.terminal_output.push(format!("[CPU EXCEPTION: Vector {}] {}", vector, desc));
        self.terminal_output.push(format!("Line {}: {}", line_num + 1, code));
    }

    pub fn resolve_value(&self, token: &str) -> Result<i32, String> {
        let mut t = token.trim();
        while t.starts_with('(') && t.ends_with(')') && Self::is_matching_outer_parens(t) {
            t = t[1..t.len() - 1].trim();
        }

        if let Some(r) = Register::from_str(t) {
            return Ok(self.reg(r));
        }

        if t.starts_with('[') && t.ends_with(']') {
            let inner = &t[1..t.len() - 1];
            let addr = self.resolve_value(inner)?;
            if addr >= 0 {
                return self.ram.read(addr as usize);
            }
            return Err(format!("RAM address {} negative or invalid", addr));
        }

        // Binary operations in precedence order: |, ^, &, <<, >>, +, -, *, /, %
        if let Some(idx) = Self::find_binary_op(t, &["|"]) {
            let left = self.resolve_value(&t[..idx])?;
            let right = self.resolve_value(&t[idx + 1..])?;
            return Ok(left | right);
        }
        if let Some(idx) = Self::find_binary_op(t, &["^"]) {
            let left = self.resolve_value(&t[..idx])?;
            let right = self.resolve_value(&t[idx + 1..])?;
            return Ok(left ^ right);
        }
        if let Some(idx) = Self::find_binary_op(t, &["&"]) {
            let left = self.resolve_value(&t[..idx])?;
            let right = self.resolve_value(&t[idx + 1..])?;
            return Ok(left & right);
        }
        if let Some(idx) = Self::find_binary_op(t, &["<<", ">>"]) {
            if t[idx..].starts_with("<<") {
                let left = self.resolve_value(&t[..idx])?;
                let right = self.resolve_value(&t[idx + 2..])?;
                return Ok(left << right);
            } else {
                let left = self.resolve_value(&t[..idx])?;
                let right = self.resolve_value(&t[idx + 2..])?;
                return Ok(left >> right);
            }
        }
        if let Some(idx) = Self::find_binary_op(t, &["+", "-"]) {
            let op = t.chars().nth(idx).unwrap();
            let left = self.resolve_value(&t[..idx])?;
            let right = self.resolve_value(&t[idx + 1..])?;
            return Ok(if op == '+' { left + right } else { left - right });
        }
        if let Some(idx) = Self::find_binary_op(t, &["*", "/", "%"]) {
            let op = t.chars().nth(idx).unwrap();
            let left = self.resolve_value(&t[..idx])?;
            let right = self.resolve_value(&t[idx + 1..])?;
            return match op {
                '*' => Ok(left * right),
                '/' => {
                    if right == 0 {
                        Err(format!("Division by zero in expression: {}", t))
                    } else {
                        Ok(left / right)
                    }
                }
                '%' => {
                    if right == 0 {
                        Err(format!("Modulo by zero in expression: {}", t))
                    } else {
                        Ok(left % right)
                    }
                }
                _ => unreachable!(),
            };
        }

        Assembler::parse_literal(t, &self.labels)
    }

    pub fn set_destination(&mut self, dest: &str, val: i32) -> Result<(), String> {
        let mut d = dest.trim();
        while d.starts_with('(') && d.ends_with(')') && Self::is_matching_outer_parens(d) {
            d = d[1..d.len() - 1].trim();
        }

        if let Some(r) = Register::from_str(d) {
            self.set_reg(r, val);
            return Ok(());
        }

        if d.starts_with('[') && d.ends_with(']') {
            let inner = &d[1..d.len() - 1];
            let addr = self.resolve_value(inner)?;
            if addr >= 0 {
                return self.ram.write(addr as usize, val);
            }
            return Err(format!("RAM address {} invalid", addr));
        }

        let lower = d.to_lowercase();
        if let Some(&addr) = self.data_labels.get(&lower) {
            return self.ram.write(addr, val);
        }

        Err(format!("Invalid destination target: {}", dest))
    }

    pub fn resolve_target(&self, target: &str) -> Result<usize, String> {
        let t = target.trim();
        if let Some(r) = Register::from_str(t) {
            let v = self.reg(r);
            return Ok(v.max(0) as usize);
        }
        if t.starts_with('[') && t.ends_with(']') {
            let inner = &t[1..t.len() - 1];
            let v = self.resolve_value(inner)?;
            return Ok(v.max(0) as usize);
        }
        let lower = t.to_lowercase();
        if let Some(&pc) = self.labels.get(&lower) {
            return Ok(pc);
        }
        if let Ok(imm) = Assembler::parse_literal(t, &self.labels) {
            return Ok(imm.max(0) as usize);
        }
        Err(format!("Jump target or label not found: {}", target))
    }

    fn parse_vector(&self, token: &str) -> Result<i32, String> {
        match token.trim().to_uppercase().as_str() {
            "DIV_ZERO" => Ok(VEC_DIV_ZERO),
            "ILLEGAL_OPCODE" => Ok(VEC_ILLEGAL_OPCODE),
            "OUT_OF_BOUNDS" => Ok(VEC_OUT_OF_BOUNDS),
            "STACK_FAULT" => Ok(VEC_STACK_FAULT),
            "GENERAL_EXCEPTION" | "EXCEPTION" | "ERROR" => Ok(VEC_GENERAL_EXCEPTION),
            "KEYBOARD" | "KEY" | "CHAR" => Ok(VEC_KEYBOARD),
            "MOUSE_MOVE" | "MOUSE_POS" => Ok(VEC_MOUSE_MOVE),
            "MOUSE_CLICK" | "CLICK" => Ok(VEC_MOUSE_CLICK),
            "TIMER" | "VBLANK" | "VBL" | "NMI" | "FRAME" => Ok(VEC_TIMER),
            "RESET" | "RST" => Ok(VEC_RESET),
            _ => self.resolve_value(token),
        }
    }

    pub fn step(&mut self) {
        if self.state != State::Running && self.state != State::Paused {
            return;
        }
        self.execute_next_instruction();
        self.gpu.advance();
    }

    pub fn tick(&mut self, max_cycles: usize) {
        if self.state != State::Running {
            return;
        }
        self.check_timer_trigger();
        if self.sleep_ticks_remaining > 0 {
            self.check_and_dispatch_interrupt();
            if self.sleep_ticks_remaining > 0 {
                self.sleep_ticks_remaining -= 1;
                self.gpu.advance();
                return;
            }
        }

        let mut count = 0;
        let start = Instant::now();
        while self.state == State::Running && count < max_cycles {
            self.execute_next_instruction();
            count += 1;
            if (count & 63) == 0 {
                self.check_timer_trigger();
                if start.elapsed().as_millis() > 10 {
                    break;
                }
            }
            if self.sleep_ticks_remaining > 0 {
                break;
            }
        }
        self.gpu.advance();
    }

    pub fn execute_next_instruction(&mut self) {
        self.peripheral_bus.service();
        self.network_port.service();
        self.check_and_dispatch_interrupt();

        if self.pc >= self.program.len() {
            self.state = State::Halted;
            self.flush_line_buffer();
            self.terminal_output.push("[PROGRAM TERMINATED: END OF FILE]".to_string());
            return;
        }

        if self.compiled_program.len() != self.program.len() {
            self.compile_bytecode();
        }

        let line_num = self.pc;
        let ci = self.compiled_program[self.pc].clone();
        self.pc += 1;
        self.cycles += 1;

        if ci.opcode == Opcode::NOP || ci.opcode.is_directive() {
            return;
        }

        if let Err(e) = self.exec_op(&ci, line_num) {
            let vec = if e.contains("zero") {
                VEC_DIV_ZERO
            } else if e.contains("bounds") {
                VEC_OUT_OF_BOUNDS
            } else if e.contains("underflow") || e.contains("overflow") {
                VEC_STACK_FAULT
            } else {
                VEC_GENERAL_EXCEPTION
            };
            self.trigger_exception(vec, &e, line_num, &ci.raw_line);
        }

        if self.terminal_output.len() > 100 {
            self.terminal_output.remove(0);
        }
    }

    fn submit_peripheral(&mut self, opcode: Opcode, args: &str) -> Result<(), String> {
        use peripheral::Operation;

        let parts = instruction_args(args, if matches!(opcode, Opcode::PCALL | Opcode::PCALLS) { 5 } else { 2 })?;
        let address = self.resolve_value(&parts[1])?;
        if self.peripheral_bus.status() != 0 {
            return self.set_destination(&parts[0], 0);
        }
        let operation = match opcode {
            Opcode::PINFO => Operation::Info,
            Opcode::PWRAP => Operation::Wrap,
            Opcode::PSCAN => Operation::Scan,
            Opcode::PCALL | Opcode::PCALLS => {
                let method = self.resolve_value(&parts[2])?;
                let arg0 = self.resolve_value(&parts[3])?;
                let arg1 = self.resolve_value(&parts[4])?;
                if opcode == Opcode::PCALLS {
                    Operation::CallString(method, self.read_guest_string(arg0, 256)?, arg1)
                } else {
                    Operation::Call(method, arg0, arg1)
                }
            }
            _ => unreachable!(),
        };
        self.peripheral_bus.submit(address, operation);
        self.set_destination(&parts[0], 1)
    }

    fn read_peripheral_text(&mut self, args: &str) -> Result<(), String> {
        let parts = instruction_args(args, 3)?;
        let range = self.guest_range(self.resolve_value(&parts[1])?, self.resolve_value(&parts[2])?)?;
        let text = self.peripheral_bus.take_text();
        let buffer = &mut self.ram.as_mut_slice()[range];
        let max_chars = buffer.len().saturating_sub(1);
        let mut count = 0;
        for (cell, unit) in buffer.iter_mut().take(max_chars).zip(text.encode_utf16()) {
            *cell = i32::from(unit);
            count += 1;
        }
        if let Some(terminator) = buffer.get_mut(count) {
            *terminator = 0;
        }
        self.set_destination(&parts[0], count as i32)
    }

    fn read_guest_string(&self, address: i32, limit: usize) -> Result<String, String> {
        let address = usize::try_from(address).map_err(|_| "Guest string out of bounds")?;
        let mut text = Vec::new();
        for offset in 0..=limit {
            let value = self.ram.read(address + offset)?;
            if value == 0 {
                return Ok(String::from_utf16_lossy(&text));
            }
            if offset == limit {
                break;
            }
            text.push(u16::try_from(value).map_err(|_| "Invalid character in guest string")?);
        }
        Err(format!("Guest string exceeds {limit} characters"))
    }

    fn submit_network(&mut self, opcode: Opcode, args: &str) -> Result<(), String> {
        use network::Request;

        let count = match opcode {
            Opcode::NOPEN | Opcode::NSEND | Opcode::NWSTEXT => 4,
            Opcode::NRECV | Opcode::NLISTEN => 3,
            Opcode::NCLOSE | Opcode::NWSOPEN | Opcode::NTLS | Opcode::NACCEPT => 2,
            Opcode::NHTTP | Opcode::NHTTP3 | Opcode::NRESPOND | Opcode::NRESPONDT => 5,
            _ => unreachable!(),
        };
        // NHTTP and NHTTP3 accept a trailing flag that selects a plain-text content type.
        let with_text_flag = matches!(opcode, Opcode::NHTTP | Opcode::NHTTP3)
            && Assembler::split_args(args, usize::MAX).len() == count + 1;
        let parts = instruction_args(args, count + usize::from(with_text_flag))?;
        if self.network_port.status() != 0 {
            return self.set_destination(&parts[0], 0);
        }

        let value = |i: usize| self.resolve_value(&parts[i]);
        let request = match opcode {
            Opcode::NOPEN => Request::Open(value(1)?, self.read_guest_string(value(2)?, network::MAX_TEXT)?, value(3)?),
            Opcode::NSEND | Opcode::NWSTEXT => {
                Request::Send(value(1)?, self.network_payload(value(2)?, value(3)?)?, opcode == Opcode::NWSTEXT)
            }
            Opcode::NRECV => Request::Receive(value(1)?, value(2)?),
            Opcode::NCLOSE => Request::Close(value(1)?),
            Opcode::NHTTP | Opcode::NHTTP3 => {
                let text = with_text_flag && value(5)? != 0;
                Request::Http(
                    value(1)?,
                    self.read_guest_string(value(2)?, network::MAX_TEXT)?,
                    self.network_payload(value(3)?, value(4)?)?,
                    text,
                    opcode == Opcode::NHTTP3,
                )
            }
            Opcode::NWSOPEN => Request::WebSocket(self.read_guest_string(value(1)?, network::MAX_TEXT)?),
            Opcode::NTLS => Request::Tls(value(1)?),
            Opcode::NLISTEN => Request::Listen(value(1)?, value(2)?),
            Opcode::NACCEPT => Request::Accept(value(1)?),
            Opcode::NRESPOND | Opcode::NRESPONDT => Request::Respond(
                value(1)?,
                value(2)?,
                self.network_payload(value(3)?, value(4)?)?,
                opcode == Opcode::NRESPONDT,
            ),
            _ => unreachable!(),
        };
        self.network_port.submit(request);
        self.set_destination(&parts[0], 1)
    }

    /// `NUTF8 dst, source, count, target, capacity`: convert between guest strings and UTF-8 bytes.
    fn transcode_network_text(&mut self, args: &str) -> Result<(), String> {
        const LIMIT: i32 = -4;
        let parts = instruction_args(args, 5)?;
        let source = self.resolve_value(&parts[1])?;
        let count = self.resolve_value(&parts[2])?;
        let target = self.resolve_value(&parts[3])?;
        let capacity = self.resolve_value(&parts[4])?;
        let target_range = self.guest_range(target, capacity)?;
        let limit = network::MAX_TRANSFER;
        let result = if count == -1 {
            match self.read_guest_string(source, limit) {
                Ok(text) if text.len() <= limit && (capacity == 0 || text.len() <= capacity as usize) => {
                    if capacity != 0 {
                        let slots = &mut self.ram.as_mut_slice()[target_range];
                        for (slot, byte) in slots.iter_mut().zip(text.bytes()) {
                            *slot = i32::from(byte);
                        }
                    }
                    text.len() as i32
                }
                _ => LIMIT,
            }
        } else if count < 0 || count as usize > limit {
            LIMIT
        } else {
            let bytes = self.network_payload(source, count)?;
            let units: Vec<u16> = String::from_utf8_lossy(&bytes).encode_utf16().collect();
            if capacity != 0 && units.len() >= capacity as usize {
                LIMIT
            } else {
                if capacity != 0 {
                    let slots = &mut self.ram.as_mut_slice()[target_range];
                    for (slot, unit) in slots.iter_mut().zip(units.iter().copied().chain([0])) {
                        *slot = i32::from(unit);
                    }
                }
                units.len() as i32
            }
        };
        self.set_destination(&parts[0], result)
    }

    /// `GALLOC size, base`: allocate from the managed guest heap, or initialize it when size is 0.
    fn guest_alloc(&mut self, args: &str) -> Result<(), String> {
        let parts = instruction_args(args, 2)?;
        let size = self.resolve_value(&parts[0])?;
        let base = self.resolve_value(&parts[1])?;
        if size == 0 {
            heap::initialize(self.ram.as_mut_slice(), base)?;
            self.reg_a = 0;
            return Ok(());
        }
        let mut roots = vec![self.reg_a, self.reg_b, self.reg_c, self.reg_d];
        roots.extend(self.data_stack.iter().copied());
        for context in self.interrupt_stack.iter().filter(|context| context.restore_regs) {
            roots.extend([context.reg_a, context.reg_b, context.reg_c, context.reg_d]);
        }
        self.reg_a = heap::allocate(self.ram.as_mut_slice(), base, size, &roots)?;
        Ok(())
    }

    fn guest_range(&self, address: i32, length: i32) -> Result<std::ops::Range<usize>, String> {
        if address < 0 || length < 0 || i64::from(address) + i64::from(length) > self.ram.len() as i64 {
            return Err("Guest buffer out of bounds".to_string());
        }
        let start = address as usize;
        Ok(start..start + length as usize)
    }

    fn network_payload(&self, address: i32, length: i32) -> Result<Vec<u8>, String> {
        let range = self.guest_range(address, length)?;
        if range.len() > network::MAX_TRANSFER {
            return Err("Network payload exceeds transfer limit".to_string());
        }
        Ok(self.ram.as_slice()[range].iter().map(|&word| word as u8).collect())
    }

    fn read_network_result(&mut self, args: &str) -> Result<(), String> {
        let parts = instruction_args(args, 3)?;
        let range = self.guest_range(self.resolve_value(&parts[1])?, self.resolve_value(&parts[2])?)?;
        let value = self.network_port.take_result().map_or(0, |result| {
            for (word, byte) in self.ram.as_mut_slice()[range].iter_mut().zip(result.data) {
                *word = i32::from(byte);
            }
            result.value
        });
        self.set_destination(&parts[0], value)
    }

    fn transfer_storage(&mut self, opcode: Opcode, args: &str) -> Result<(), String> {
        let parts = Assembler::split_args(args, usize::MAX);
        let write = opcode == Opcode::SBLKW;
        let value = |index: usize| self.resolve_value(&parts[index]);
        let selected = self.storage.selected_drive();
        let (drive, disk, memory, count) = match (write, parts.len()) {
            (false, 3) => (selected, value(1)?, value(0)?, value(2)?),
            (false, 4) => (value(1)? as usize, value(2)?, value(0)?, value(3)?),
            (true, 3) => (selected, value(0)?, value(1)?, value(2)?),
            (true, 4) => (value(0)? as usize, value(1)?, value(2)?, value(3)?),
            _ => return Err("Storage transfer requires three or four operands".to_string()),
        };
        let (disk, memory) = (i64::from(disk), i64::from(memory));
        let offset = 0.max(-disk).max(-memory);
        let capacity = self.storage.capacity(drive) as i64;
        let end = i64::from(count).min(capacity - disk).min(self.ram.len() as i64 - memory);
        self.flag_z = capacity == 0 || offset >= end;
        if self.flag_z {
            return Ok(());
        }
        let disk = (disk + offset) as usize;
        let memory = (memory + offset) as usize;
        let count = (end - offset) as usize;
        if write {
            self.storage.block_write(drive, disk, &self.ram, memory, count)
        } else {
            self.storage.block_read(drive, disk, &mut self.ram, memory, count)
        }
    }

    fn exec_gpu(&mut self, opcode: Opcode, args: &str) -> Result<(), String> {
        let values = |count| -> Result<Vec<i32>, String> {
            instruction_args(args, count)?.iter().map(|arg| self.resolve_value(arg)).collect()
        };
        let command = match opcode {
            Opcode::GPUCLR => GpuCommand::Clear(self.resolve_value(args)? as u32),
            Opcode::GPURECT | Opcode::GPULINE => {
                let [x, y, w, h, color]: [i32; 5] = values(5)?.try_into().unwrap();
                if opcode == Opcode::GPURECT {
                    GpuCommand::Rect([x, y, w, h], color as u32)
                } else {
                    GpuCommand::Line([x, y, w, h], color as u32)
                }
            }
            Opcode::GPUCIRC => {
                let parts = Assembler::split_args(args, usize::MAX);
                if !(4..=5).contains(&parts.len()) {
                    return Err("GPUCIRC requires cx, cy, radius, color [, filled]".into());
                }
                let value = |i: usize| self.resolve_value(&parts[i]);
                let filled = parts.len() == 4 || value(4)? != 0;
                GpuCommand::Circle(value(0)?, value(1)?, value(2)?, value(3)? as u32, filled)
            }
            Opcode::GPUTRI => {
                let range = self.guest_range(self.resolve_value(args)?, 7)?;
                let &[x0, y0, x1, y1, x2, y2, color] =
                    <&[i32; 7]>::try_from(&self.ram.as_slice()[range]).unwrap();
                GpuCommand::Triangle([(x0, y0), (x1, y1), (x2, y2)], color as u32)
            }
            Opcode::GPUVRAM => {
                let [offset, source, count]: [i32; 3] = values(3)?.try_into().unwrap();
                return self.gpu.upload(offset, source, count, &self.ram);
            }
            Opcode::GPUBLIT => {
                let [x, y, offset, w, h]: [i32; 5] = values(5)?.try_into().unwrap();
                return self.gpu.blit([x, y, w, h], offset);
            }
            Opcode::GPUSWAP => {
                self.framebuffer.set_mode(1);
                self.gpu.submit(GpuCommand::Swap);
                self.gpu.flush();
                return Ok(());
            }
            Opcode::GPUSYNC => {
                self.gpu.flush();
                return Ok(());
            }
            _ => unreachable!(),
        };
        self.gpu.submit(command);
        Ok(())
    }

    fn exec_op(&mut self, ci: &CompiledInstruction, line_num: usize) -> Result<(), String> {
        match ci.opcode {
            Opcode::NOP => Ok(()),
            Opcode::HLT => {
                self.state = State::Halted;
                self.flush_line_buffer();
                self.terminal_output.push("[EXECUTION HALTED]".to_string());
                Ok(())
            }
            Opcode::MOV => {
                let val = match &ci.src {
                    Operand::Reg(r) => self.reg(*r),
                    Operand::Imm(v) => *v,
                    Operand::RamImm(addr) => self.ram.read(*addr)?,
                    Operand::RamReg(r) => {
                        let a = self.reg(*r);
                        if a >= 0 {
                            self.ram.read(a as usize)?
                        } else {
                            return Err(format!("RAM address {} invalid", a));
                        }
                    }
                    _ => {
                        let parts = Assembler::split_args(&ci.raw_args, 2);
                        if parts.len() != 2 {
                            return Err("MOV requires dest, src".to_string());
                        }
                        self.resolve_value(&parts[1])?
                    }
                };

                match &ci.dest {
                    Operand::Reg(r) => {
                        self.set_reg(*r, val);
                        Ok(())
                    }
                    Operand::RamImm(addr) => self.ram.write(*addr, val),
                    Operand::RamReg(r) => {
                        let a = self.reg(*r);
                        if a >= 0 {
                            self.ram.write(a as usize, val)
                        } else {
                            Err(format!("RAM address {} invalid", a))
                        }
                    }
                    _ => {
                        let parts = Assembler::split_args(&ci.raw_args, 2);
                        self.set_destination(&parts[0], val)
                    }
                }
            }
            Opcode::ADD => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("ADD requires dest, src".to_string());
                }
                let cur = self.resolve_value(&parts[0])?;
                let addend = self.resolve_value(&parts[1])?;
                let res = cur.wrapping_add(addend);
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::SUB => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("SUB requires dest, src".to_string());
                }
                let cur = self.resolve_value(&parts[0])?;
                let subtrahend = self.resolve_value(&parts[1])?;
                let res = cur.wrapping_sub(subtrahend);
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::MUL => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("MUL requires dest, src".to_string());
                }
                let cur = self.resolve_value(&parts[0])?;
                let mul = self.resolve_value(&parts[1])?;
                let res = cur.wrapping_mul(mul);
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::DIV => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("DIV requires dest, src".to_string());
                }
                let cur = self.resolve_value(&parts[0])?;
                let divisor = self.resolve_value(&parts[1])?;
                if divisor == 0 {
                    return Err("Division by zero in DIV".to_string());
                }
                let res = cur / divisor;
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::MOD => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("MOD requires dest, src".to_string());
                }
                let cur = self.resolve_value(&parts[0])?;
                let divisor = self.resolve_value(&parts[1])?;
                if divisor == 0 {
                    return Err("Modulo by zero in MOD".to_string());
                }
                let res = cur % divisor;
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::INC => {
                let arg = ci.raw_args.trim();
                let cur = self.resolve_value(arg)?;
                let res = cur.wrapping_add(1);
                self.set_destination(arg, res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::DEC => {
                let arg = ci.raw_args.trim();
                let cur = self.resolve_value(arg)?;
                let res = cur.wrapping_sub(1);
                self.set_destination(arg, res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::CMP => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("CMP requires dest, src".to_string());
                }
                let left = self.resolve_value(&parts[0])?;
                let right = self.resolve_value(&parts[1])?;
                let diff = left.wrapping_sub(right);
                self.update_flags(diff);
                Ok(())
            }
            Opcode::AND => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("AND requires dest, src".to_string()); }
                let a = self.resolve_value(&parts[0])?;
                let b = self.resolve_value(&parts[1])?;
                let res = a & b;
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::OR => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("OR requires dest, src".to_string()); }
                let a = self.resolve_value(&parts[0])?;
                let b = self.resolve_value(&parts[1])?;
                let res = a | b;
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::XOR => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("XOR requires dest, src".to_string()); }
                let a = self.resolve_value(&parts[0])?;
                let b = self.resolve_value(&parts[1])?;
                let res = a ^ b;
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::NOT => {
                let arg = ci.raw_args.trim();
                let a = self.resolve_value(arg)?;
                let res = !a;
                self.set_destination(arg, res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::SHL => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("SHL requires dest, count".to_string()); }
                let a = self.resolve_value(&parts[0])?;
                let shift = self.resolve_value(&parts[1])?;
                let res = a << (shift & 31);
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::SHR => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("SHR requires dest, count".to_string()); }
                let a = self.resolve_value(&parts[0])?;
                let shift = self.resolve_value(&parts[1])?;
                let res = a >> (shift & 31);
                self.set_destination(&parts[0], res)?;
                self.update_flags(res);
                Ok(())
            }
            Opcode::JMP => self.exec_jmp(&ci.raw_args, ci.target_pc, true),
            Opcode::JZ => self.exec_jmp(&ci.raw_args, ci.target_pc, self.flag_z),
            Opcode::JNZ => self.exec_jmp(&ci.raw_args, ci.target_pc, !self.flag_z),
            Opcode::JS => self.exec_jmp(&ci.raw_args, ci.target_pc, self.flag_s),
            Opcode::JNS => self.exec_jmp(&ci.raw_args, ci.target_pc, !self.flag_s),
            Opcode::JL => self.exec_jmp(&ci.raw_args, ci.target_pc, self.flag_s && !self.flag_z),
            Opcode::JLE => self.exec_jmp(&ci.raw_args, ci.target_pc, self.flag_s || self.flag_z),
            Opcode::JG => self.exec_jmp(&ci.raw_args, ci.target_pc, !self.flag_s && !self.flag_z),
            Opcode::JGE => self.exec_jmp(&ci.raw_args, ci.target_pc, !self.flag_s),
            Opcode::CALL => {
                let dest = if let Some(target) = ci.target_pc {
                    target
                } else {
                    self.resolve_target(&ci.raw_args)?
                };
                self.call_stack.push(self.pc);
                self.pc = dest.min(self.program.len());
                Ok(())
            }
            Opcode::RET => {
                if let Some(target) = self.call_stack.pop() {
                    self.pc = target;
                    Ok(())
                } else {
                    Err("RET without CALL: Call stack underflow".to_string())
                }
            }
            Opcode::PUSH => {
                let val = match &ci.dest {
                    Operand::Reg(r) => self.reg(*r),
                    Operand::Imm(imm) => *imm,
                    _ => self.resolve_value(&ci.raw_args)?,
                };
                self.data_stack.push(val);
                Ok(())
            }
            Opcode::POP => {
                if let Some(val) = self.data_stack.pop() {
                    match &ci.dest {
                        Operand::Reg(r) => {
                            self.set_reg(*r, val);
                            Ok(())
                        }
                        _ => self.set_destination(&ci.raw_args, val),
                    }
                } else {
                    Err("POP on empty stack: Data stack underflow".to_string())
                }
            }
            Opcode::PUSHA => {
                self.data_stack.push(self.reg_a);
                self.data_stack.push(self.reg_b);
                self.data_stack.push(self.reg_c);
                self.data_stack.push(self.reg_d);
                Ok(())
            }
            Opcode::POPA => {
                if self.data_stack.len() < 4 {
                    return Err("Data stack underflow on POPA".to_string());
                }
                self.reg_d = self.data_stack.pop().unwrap();
                self.reg_c = self.data_stack.pop().unwrap();
                self.reg_b = self.data_stack.pop().unwrap();
                self.reg_a = self.data_stack.pop().unwrap();
                Ok(())
            }
            Opcode::IVEC => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 {
                    return Err("IVEC requires vector, handler".to_string());
                }
                let vec = self.parse_vector(&parts[0])?;
                let target = self.resolve_target(&parts[1])?;
                self.ivt.insert(vec, target);
                Ok(())
            }
            Opcode::IRET => {
                if let Some(ctx) = self.interrupt_stack.pop() {
                    self.pc = ctx.pc;
                    if ctx.restore_regs {
                        self.reg_a = ctx.reg_a;
                        self.reg_b = ctx.reg_b;
                        self.reg_c = ctx.reg_c;
                        self.reg_d = ctx.reg_d;
                    }
                    self.flag_z = ctx.flag_z;
                    self.flag_s = ctx.flag_s;
                    self.in_interrupt_handler = !self.interrupt_stack.is_empty();
                    Ok(())
                } else {
                    Err("IRET with empty interrupt stack: Stack underflow".to_string())
                }
            }
            Opcode::CLI => {
                self.interrupts_enabled = false;
                Ok(())
            }
            Opcode::STI => {
                self.interrupts_enabled = true;
                Ok(())
            }
            Opcode::INT => {
                let vec = self.parse_vector(&ci.raw_args)?;
                if let Some(&handler_pc) = self.ivt.get(&vec) {
                    self.interrupt_stack.push(CpuContext {
                        pc: self.pc,
                        reg_a: self.reg_a,
                        reg_b: self.reg_b,
                        reg_c: self.reg_c,
                        reg_d: self.reg_d,
                        flag_z: self.flag_z,
                        flag_s: self.flag_s,
                        restore_regs: false,
                    });
                    self.in_interrupt_handler = true;
                    self.reg_a = vec;
                    self.pc = handler_pc;
                    Ok(())
                } else {
                    self.trigger_exception(vec, &format!("Software interrupt {}", vec), line_num, &ci.raw_line);
                    Ok(())
                }
            }
            Opcode::OUT => self.exec_out(&ci.raw_args),
            Opcode::OUTH => {
                let val = self.resolve_value(&ci.raw_args)?;
                self.line_buffer.push_str(&format!("0x{:X}", val));
                self.flush_line_buffer();
                Ok(())
            }
            Opcode::OUTC => {
                let arg = ci.raw_args.trim();
                if let Some(text) = output_string(arg) {
                    for byte in text.bytes() {
                        self.emit_character(i32::from(byte));
                    }
                } else {
                    self.emit_character(self.resolve_value(arg)?);
                }
                Ok(())
            }
            Opcode::CLS => {
                self.terminal_output.clear();
                self.line_buffer.clear();
                self.utf8_bytes.clear();
                Ok(())
            }
            Opcode::SLEEP => {
                let ticks = self.resolve_value(&ci.raw_args)?;
                self.sleep_ticks_remaining = ticks.max(0);
                Ok(())
            }
            Opcode::TIME => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i32;
                self.set_destination(&ci.raw_args, now)
            }
            Opcode::RAND => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                let max = if parts.len() >= 2 {
                    self.resolve_value(&parts[1])?.max(1)
                } else {
                    i32::MAX
                };
                let r = (std::time::SystemTime::now().elapsed().unwrap_or_default().as_nanos() % (max as u128)) as i32;
                self.set_destination(&parts[0], r)
            }
            Opcode::INPAD => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                let dest = parts.first().ok_or("INPAD requires a destination")?;
                let port = parts.get(1).map_or(Ok(0), |port| self.resolve_value(port))?;
                let value = match port {
                    -1 => self.key_buffer.pop_front().unwrap_or(-1),
                    -2 => self.key_buffer.front().copied().unwrap_or(-1),
                    0..=3 => self.pad_states[port as usize],
                    _ => 0,
                };
                self.set_destination(dest, value)
            }
            Opcode::INAXIS => {
                let parts = instruction_args(&ci.raw_args, 3)?;
                let pad = self.resolve_value(&parts[1])? as usize;
                let axis = self.resolve_value(&parts[2])? as usize;
                let value = self.pad_axes.get(pad).and_then(|axes| axes.get(axis)).copied().unwrap_or(0);
                self.set_destination(&parts[0], value)
            }
            Opcode::PINFO | Opcode::PWRAP | Opcode::PCALL | Opcode::PCALLS | Opcode::PSCAN =>
                self.submit_peripheral(ci.opcode, &ci.raw_args),
            Opcode::PSTATUS => self.set_destination(&ci.raw_args, self.peripheral_bus.status()),
            Opcode::PLEN => self.set_destination(&ci.raw_args, self.peripheral_bus.text_length()),
            Opcode::PRESULTS => self.read_peripheral_text(&ci.raw_args),
            Opcode::PRESULT => {
                let value = self.peripheral_bus.take_result();
                self.set_destination(&ci.raw_args, value)
            }
            Opcode::NOPEN | Opcode::NSEND | Opcode::NRECV | Opcode::NCLOSE | Opcode::NHTTP
            | Opcode::NWSOPEN | Opcode::NWSTEXT | Opcode::NTLS | Opcode::NHTTP3
            | Opcode::NLISTEN | Opcode::NACCEPT | Opcode::NRESPOND | Opcode::NRESPONDT =>
                self.submit_network(ci.opcode, &ci.raw_args),
            Opcode::NREADY => {
                let ready = i32::from(self.network_port.ready());
                self.set_destination(&ci.raw_args, ready)
            }
            Opcode::NUTF8 => self.transcode_network_text(&ci.raw_args),
            Opcode::GALLOC => self.guest_alloc(&ci.raw_args),
            Opcode::NSTATUS => self.set_destination(&ci.raw_args, self.network_port.status()),
            Opcode::NLEN => self.set_destination(&ci.raw_args, self.network_port.result_length()),
            Opcode::NRESULT => self.read_network_result(&ci.raw_args),
            Opcode::TIMER => {
                let hz = self.resolve_value(&ci.raw_args)?;
                self.set_timer_hz(hz);
                Ok(())
            }
            Opcode::VBLANK => {
                let arg = ci.raw_args.trim().to_uppercase();
                let hz = if arg.is_empty() || arg == "1" || arg == "TRUE" || arg == "ON" {
                    60
                } else if arg == "0" || arg == "FALSE" || arg == "OFF" {
                    0
                } else {
                    let v = self.resolve_value(&arg)?;
                    if v > 0 { 60 } else { 0 }
                };
                self.set_timer_hz(hz);
                Ok(())
            }
            Opcode::SSIZE => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                let drive = if parts.len() >= 2 {
                    self.resolve_value(&parts[1])? as usize
                } else {
                    self.storage.selected_drive()
                };
                let capacity = self.storage.capacity(drive);
                self.set_destination(&parts[0], capacity as i32)?;
                self.flag_z = capacity == 0;
                self.flag_s = false;
                Ok(())
            }
            Opcode::SREAD => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                let (dest, drive, addr) = if parts.len() >= 3 {
                    (&parts[0], self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])? as usize)
                } else if parts.len() == 2 {
                    (&parts[0], self.storage.selected_drive(), self.resolve_value(&parts[1])? as usize)
                } else {
                    return Err("SREAD requires dest, [drive,] addr".to_string());
                };
                self.flag_s = addr >= self.storage.capacity(drive);
                let value = if self.flag_s { 0 } else { self.storage.read_byte(drive, addr)? };
                self.flag_z = value == 0;
                self.set_destination(dest, value)
            }
            Opcode::SWRITE => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                let (drive, addr, val) = if parts.len() >= 3 {
                    (self.resolve_value(&parts[0])? as usize, self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])?)
                } else if parts.len() == 2 {
                    (self.storage.selected_drive(), self.resolve_value(&parts[0])? as usize, self.resolve_value(&parts[1])?)
                } else {
                    return Err("SWRITE requires [drive,] addr, val".to_string());
                };
                self.flag_z = addr >= self.storage.capacity(drive);
                self.flag_s = self.flag_z;
                if !self.flag_s {
                    self.storage.write_byte(drive, addr, val)?;
                }
                Ok(())
            }
            Opcode::SBLKR | Opcode::SBLKW => self.transfer_storage(ci.opcode, &ci.raw_args),
            Opcode::SCOUNT => {
                let count = self.storage.drive_count() as i32;
                self.set_destination(&ci.raw_args, count)?;
                self.flag_z = count == 0;
                self.flag_s = false;
                Ok(())
            }
            Opcode::SSEL => {
                let drive = self.resolve_value(&ci.raw_args)? as usize;
                self.flag_s = self.storage.select_drive(drive).is_err();
                self.flag_z = self.storage.capacity(drive) == 0;
                Ok(())
            }
            Opcode::SDRV => {
                let drive = self.storage.selected_drive() as i32;
                self.set_destination(&ci.raw_args, drive)?;
                self.flag_z = drive == 0;
                self.flag_s = false;
                Ok(())
            }
            Opcode::SEXEC => {
                let drive = self.resolve_value(&ci.raw_args)? as usize;
                if drive >= crate::hardware::storage::MAX_DRIVES {
                    return Err(format!("Drive index {drive} out of bounds"));
                }
                let program = self.storage.get_drive_program(drive).cloned()
                    .or_else(|| self.storage.read_program_text(drive))
                    .ok_or_else(|| format!("No executable program found on drive {drive}"))?;
                self.network_port.reset();
                self.peripheral_bus.reset();
                self.program = program;
                self.preprocess_labels();
                self.compile_bytecode();
                self.call_stack.clear();
                self.pc = 0;
                self.storage.select_drive(drive)?;
                self.flag_z = false;
                self.flag_s = false;
                self.flush_line_buffer();
                Ok(())
            }
            Opcode::GFXSEL => {
                self.gpu.handler = match self.resolve_value(&ci.raw_args)? {
                    mode @ (1 | 2) => mode,
                    _ => 0,
                };
                self.flag_z = !self.gpu.selected();
                self.flag_s = false;
                Ok(())
            }
            Opcode::GFXSTAT => {
                let parts = Assembler::split_args(&ci.raw_args, usize::MAX);
                let (dest, query) = match parts.as_slice() {
                    [dest] => (dest, 0),
                    [dest, query] => (dest, self.resolve_value(query)?),
                    _ => return Err("GFXSTAT requires dest [, query]".into()),
                };
                let result = self.gpu.status(query);
                self.set_destination(dest, result)?;
                self.flag_z = result == 0;
                self.flag_s = result < 0;
                Ok(())
            }
            Opcode::GPUCLR | Opcode::GPURECT | Opcode::GPULINE | Opcode::GPUCIRC
            | Opcode::GPUTRI | Opcode::GPUVRAM | Opcode::GPUBLIT | Opcode::GPUSWAP
            | Opcode::GPUSYNC => self.exec_gpu(ci.opcode, &ci.raw_args),
            Opcode::FBMODE => {
                let mode = self.resolve_value(&ci.raw_args)?;
                self.framebuffer.set_mode(mode);
                self.gpu.front.set_mode(mode);
                Ok(())
            }
            Opcode::FBSET => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                if parts.len() != 3 { return Err("FBSET requires x, y, color".to_string()); }
                let x = self.resolve_value(&parts[0])?;
                let y = self.resolve_value(&parts[1])?;
                let color = self.resolve_value(&parts[2])? as u32;
                if self.gpu.selected() {
                    self.gpu.submit(GpuCommand::Pixel(x, y, color));
                } else {
                    self.framebuffer.set_pixel(x, y, color);
                }
                Ok(())
            }
            Opcode::FBGET => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                if parts.len() != 3 { return Err("FBGET requires dest, x, y".to_string()); }
                let x = self.resolve_value(&parts[1])?;
                let y = self.resolve_value(&parts[2])?;
                let color = if self.gpu.selected() {
                    self.gpu.read_back(x, y)
                } else {
                    self.framebuffer.get_pixel(x, y)
                } as i32;
                self.set_destination(&parts[0], color)
            }
            Opcode::FBCLR => {
                let color = if !ci.raw_args.trim().is_empty() {
                    self.resolve_value(&ci.raw_args)? as u32
                } else {
                    0xFF051009
                };
                if self.gpu.selected() {
                    self.gpu.submit(GpuCommand::Clear(color));
                } else {
                    self.framebuffer.clear(color);
                }
                Ok(())
            }
            Opcode::FBRECT => {
                let parts = Assembler::split_args(&ci.raw_args, 5);
                if parts.len() != 5 { return Err("FBRECT requires x, y, w, h, color".to_string()); }
                let x = self.resolve_value(&parts[0])?;
                let y = self.resolve_value(&parts[1])?;
                let w = self.resolve_value(&parts[2])?;
                let h = self.resolve_value(&parts[3])?;
                let color = self.resolve_value(&parts[4])? as u32;
                if self.gpu.selected() {
                    self.gpu.submit(GpuCommand::Rect([x, y, w, h], color));
                } else {
                    self.framebuffer.fill_rect(x, y, w, h, color);
                }
                Ok(())
            }
            Opcode::FBLINE => {
                let parts = Assembler::split_args(&ci.raw_args, 5);
                if parts.len() != 5 { return Err("FBLINE requires x1, y1, x2, y2, color".to_string()); }
                let x0 = self.resolve_value(&parts[0])?;
                let y0 = self.resolve_value(&parts[1])?;
                let x1 = self.resolve_value(&parts[2])?;
                let y1 = self.resolve_value(&parts[3])?;
                let color = self.resolve_value(&parts[4])? as u32;
                if self.gpu.selected() {
                    self.gpu.submit(GpuCommand::Line([x0, y0, x1, y1], color));
                } else {
                    self.framebuffer.draw_line(x0, y0, x1, y1, color);
                }
                Ok(())
            }
            Opcode::FBLIT => {
                let parts = Assembler::split_args(&ci.raw_args, 5);
                if parts.len() != 5 { return Err("FBLIT requires x, y, ram_src, w, h".to_string()); }
                let x = self.resolve_value(&parts[0])?;
                let y = self.resolve_value(&parts[1])?;
                let ram_src = self.resolve_value(&parts[2])?;
                let w = self.resolve_value(&parts[3])?;
                let h = self.resolve_value(&parts[4])?;
                if self.gpu.selected() {
                    self.gpu.blit_ram([x, y, w, h], ram_src, &self.ram);
                } else {
                    self.framebuffer.blit(x, y, w, h, &self.ram, ram_src as usize);
                }
                Ok(())
            }
            Opcode::FBSWAP => {
                self.framebuffer.set_mode(1);
                self.gpu.front.set_mode(1);
                if self.gpu.selected() {
                    self.gpu.submit(GpuCommand::Swap);
                    self.gpu.flush();
                }
                Ok(())
            }
            Opcode::FBWIDTH => self.set_destination(&ci.raw_args, crate::hardware::framebuffer::FB_WIDTH as i32),
            Opcode::FBHEIGHT => self.set_destination(&ci.raw_args, crate::hardware::framebuffer::FB_HEIGHT as i32),
            Opcode::APUSH => {
                let sample = (self.resolve_value(&ci.raw_args)? & 0xFF) as u8;
                self.audio.push_sample(sample);
                Ok(())
            }
            Opcode::ABUFW => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("ABUFW requires ram_src, count".to_string()); }
                let ram_src = i64::from(self.resolve_value(&parts[0])?);
                let count = i64::from(self.resolve_value(&parts[1])?);
                let start = ram_src.max(0);
                let end = (ram_src + count).min(self.ram.len() as i64);
                if start < end {
                    self.audio.write_buffer(&self.ram, start as usize, (end - start) as usize);
                }
                Ok(())
            }
            Opcode::ACLR => {
                self.audio.clear();
                Ok(())
            }
            Opcode::ASTAT => {
                let count = self.audio.stat() as i32;
                self.set_destination(&ci.raw_args, count)
            }
            Opcode::ATONE => {
                let parts = Assembler::split_args(&ci.raw_args, 2);
                if parts.len() != 2 { return Err("ATONE requires freq, duration_ms".to_string()); }
                let freq = self.resolve_value(&parts[0])?;
                let duration = self.resolve_value(&parts[1])?;
                self.audio.generate_tone(freq, duration);
                Ok(())
            }
            Opcode::APLAY => {
                self.audio.trigger_play();
                Ok(())
            }
            _ => Err(format!("Unknown or unhandled opcode: {:?}", ci.opcode)),
        }
    }

    fn exec_jmp(&mut self, args: &str, target_pc: Option<usize>, cond: bool) -> Result<(), String> {
        if !cond {
            return Ok(());
        }
        let dest = if let Some(t) = target_pc {
            t
        } else {
            self.resolve_target(args)?
        };
        self.pc = dest.min(self.program.len());
        Ok(())
    }

    fn exec_out(&mut self, args: &str) -> Result<(), String> {
        let trimmed = args.trim();
        let mut newline = true;
        let mut val_part = trimmed;

        let parts = Assembler::split_args(trimmed, 2);
        if parts.len() == 2 {
            val_part = parts[0].trim();
            let flag = parts[1].trim().to_lowercase();
            if flag == "0" || flag == "cont" || flag == "no_nl" || flag == "false" {
                newline = false;
            }
        }

        self.flush_utf8();
        if let Some(text) = output_string(val_part) {
            self.line_buffer.push_str(&text);
        } else {
            let val = self.resolve_value(val_part)?;
            self.line_buffer.push_str(&val.to_string());
        }

        if newline {
            self.flush_line_buffer();
        }
        Ok(())
    }

    fn emit_character(&mut self, value: i32) {
        match value {
            10 => {
                self.flush_utf8();
                if self.line_buffer.is_empty() {
                    self.terminal_output.push(String::new());
                } else {
                    self.flush_line_buffer();
                }
            }
            13 | i32::MIN..=-1 => {}
            128..=255 => {
                self.utf8_bytes.push(value as u8);
                if let Ok(text) = std::str::from_utf8(&self.utf8_bytes) {
                    self.line_buffer.push_str(text);
                    self.utf8_bytes.clear();
                } else if self.utf8_bytes.len() == 4 {
                    self.flush_utf8();
                }
            }
            _ => {
                self.flush_utf8();
                self.line_buffer.push(char::from_u32(value as u32).unwrap_or('?'));
            }
        }
        if self.line_buffer.encode_utf16().count() >= 42 {
            self.flush_line_buffer();
        }
    }

    fn flush_utf8(&mut self) {
        if !self.utf8_bytes.is_empty() {
            self.line_buffer.push('?');
            self.utf8_bytes.clear();
        }
    }

    pub fn flush_line_buffer(&mut self) {
        self.flush_utf8();
        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            self.terminal_output.push(line);
        }
    }

    fn is_matching_outer_parens(s: &str) -> bool {
        if !s.starts_with('(') || !s.ends_with(')') {
            return false;
        }
        let mut depth = 0;
        let chars: Vec<char> = s.chars().collect();
        for i in 0..chars.len() - 1 {
            if chars[i] == '(' {
                depth += 1;
            } else if chars[i] == ')' {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
        }
        depth == 1
    }

    fn find_binary_op(s: &str, ops: &[&str]) -> Option<usize> {
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        let chars: Vec<char> = s.chars().collect();

        // Scan right to left for left-associativity
        for i in (0..chars.len()).rev() {
            let c = chars[i];
            if (c == '\"' || c == '\'') && (i == 0 || chars[i - 1] != '\\') {
                if in_quotes && c == quote_char {
                    in_quotes = false;
                } else if !in_quotes {
                    in_quotes = true;
                    quote_char = c;
                }
            }
            if !in_quotes {
                if c == ')' { paren_depth += 1; }
                else if c == '(' { paren_depth = (paren_depth - 1).max(0); }
                else if c == ']' { bracket_depth += 1; }
                else if c == '[' { bracket_depth = (bracket_depth - 1).max(0); }
                else if paren_depth == 0 && bracket_depth == 0 {
                    for &op in ops {
                        let op_len = op.len();
                        if i >= op_len - 1 {
                            let start = i + 1 - op_len;
                            if s[start..].starts_with(op) {
                                // Do not confuse negative sign with binary minus: e.g. at start or after operator
                                if op == "-" && (start == 0 || "+-*/%&|^<>=,".contains(chars[start - 1])) {
                                    continue;
                                }
                                return Some(start);
                            }
                        }
                    }
                }
            }
        }
        None
    }
}


fn instruction_args(args: &str, expected: usize) -> Result<Vec<String>, String> {
    let parts = Assembler::split_args(args, usize::MAX);
    if parts.len() != expected || parts.iter().any(String::is_empty) {
        return Err(format!("Expected {expected} instruction operands"));
    }
    Ok(parts)
}


fn output_string(arg: &str) -> Option<String> {
    let quote = arg.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let text = arg.strip_prefix(quote)?.strip_suffix(quote)?;
    Some(text.replace("\\n", "\n").replace("\\t", "\t").replace("\\r", "\r")
        .replace("\\\"", "\"").replace("\\'", "'"))
}
