pub mod n_compiler;
pub mod project;
pub mod instruction;
pub mod assembler;

use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use instruction::{CompiledInstruction, Opcode, Operand, Register};
use assembler::Assembler;
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

    pub pad_states: [i32; 4],
    pub terminal_output: Vec<String>,
    pub line_buffer: String,

    pub ram: Ram,
    pub storage: StorageSubsystem,
    pub framebuffer: Framebuffer,
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

            pad_states: [0; 4],
            terminal_output: Vec::new(),
            line_buffer: String::new(),

            ram: Ram::new(ram_words),
            storage: StorageSubsystem::new(),
            framebuffer: Framebuffer::new(),
            audio: AudioSubsystem::new(),

            rom: Vec::new(),
            program: Vec::new(),
            compiled_program: Vec::new(),
            labels: HashMap::new(),
            data_labels: HashMap::new(),
            supported_instructions: {
                let mut set = std::collections::HashSet::new();
                for instr in [
                    "NOP", "MOV", "ADD", "SUB", "MUL", "DIV", "MOD", "INC", "DEC", "CMP",
                    "AND", "OR", "XOR", "NOT", "SHL", "SHR", "JMP", "JZ", "JNZ", "JS",
                    "JNS", "JL", "JLE", "JG", "JGE", "CALL", "RET", "PUSH", "POP", "PUSHA",
                    "POPA", "IVEC", "IRET", "CLI", "STI", "INT", "OUT", "OUTH", "OUTC",
                    "CLS", "SLEEP", "TIME", "RAND", "INPAD", "TIMER", "SETTIMER", "VBLANK",
                    "SSIZE", "SREAD", "SWRITE", "SBLKR", "SBLKW", "SCOUNT", "SSEL", "SDRV",
                    "SEXEC", "SBOOT", "FBMODE", "DMODE", "FBSET", "PIX", "FBGET", "FBCLR",
                    "FCLR", "FBRECT", "RECT", "FBLINE", "LINE", "FBLIT", "FBSWAP", "FBWIDTH",
                    "FBHEIGHT", "APUSH", "ABUFW", "ACLR", "ASTAT", "ATONE", "TONE", "APLAY",
                    "HLT", "ORG", "DB", "DW", "DL", "INCBIN"
                ] {
                    set.insert(instr.to_string());
                }
                set
            },
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
        for instr in [
            "NOP", "MOV", "ADD", "SUB", "MUL", "DIV", "MOD", "INC", "DEC", "CMP",
            "AND", "OR", "XOR", "NOT", "SHL", "SHR", "JMP", "JZ", "JNZ", "JS",
            "JNS", "JL", "JLE", "JG", "JGE", "CALL", "RET", "PUSH", "POP", "PUSHA",
            "POPA", "IVEC", "IRET", "CLI", "STI", "INT", "OUT", "OUTH", "OUTC",
            "CLS", "SLEEP", "TIME", "RAND", "INPAD", "TIMER", "SETTIMER", "VBLANK",
            "SSIZE", "SREAD", "SWRITE", "SBLKR", "SBLKW", "SCOUNT", "SSEL", "SDRV",
            "SEXEC", "SBOOT", "FBMODE", "DMODE", "FBSET", "PIX", "FBGET", "FBCLR",
            "FCLR", "FBRECT", "RECT", "FBLINE", "LINE", "FBLIT", "FBSWAP", "FBWIDTH",
            "FBHEIGHT", "APUSH", "ABUFW", "ACLR", "ASTAT", "ATONE", "TONE", "APLAY",
            "HLT", "ORG", "DB", "DW", "DL", "INCBIN"
        ] {
            self.supported_instructions.insert(instr.to_string());
        }
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
        self.terminal_output.clear();
        self.line_buffer.clear();
        self.framebuffer.clear(0xFF051009);
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
        if pad_idx < 4 {
            self.pad_states[pad_idx] = mask;
        }
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
    }

    pub fn execute_next_instruction(&mut self) {
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
                let val = self.resolve_value(&ci.raw_args)?;
                let ch = std::char::from_u32(val as u32).unwrap_or('?');
                if ch == '\n' {
                    self.flush_line_buffer();
                } else {
                    self.line_buffer.push(ch);
                }
                Ok(())
            }
            Opcode::CLS => {
                self.terminal_output.clear();
                self.line_buffer.clear();
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
                let port = if parts.len() >= 2 {
                    self.resolve_value(&parts[1])?.clamp(0, 3) as usize
                } else {
                    0
                };
                let mask = self.pad_states[port];
                self.set_destination(&parts[0], mask)
            }
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
                let cap = self.storage.capacity(drive) as i32;
                self.set_destination(&parts[0], cap)
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
                let val = self.storage.read_byte(drive, addr)?;
                self.set_destination(dest, val)
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
                self.storage.write_byte(drive, addr, val)
            }
            Opcode::SBLKR => {
                let parts = Assembler::split_args(&ci.raw_args, 4);
                let (r_dest, drive, s_addr, count) = if parts.len() >= 4 {
                    (self.resolve_value(&parts[0])? as usize, self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])? as usize, self.resolve_value(&parts[3])? as usize)
                } else if parts.len() == 3 {
                    (self.resolve_value(&parts[0])? as usize, self.storage.selected_drive(), self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])? as usize)
                } else {
                    return Err("SBLKR requires ram_dest, [drive,] storage_addr, count".to_string());
                };
                self.storage.block_read(drive, s_addr, &mut self.ram, r_dest, count)
            }
            Opcode::SBLKW => {
                let parts = Assembler::split_args(&ci.raw_args, 4);
                let (drive, s_addr, r_src, count) = if parts.len() >= 4 {
                    (self.resolve_value(&parts[0])? as usize, self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])? as usize, self.resolve_value(&parts[3])? as usize)
                } else if parts.len() == 3 {
                    (self.storage.selected_drive(), self.resolve_value(&parts[0])? as usize, self.resolve_value(&parts[1])? as usize, self.resolve_value(&parts[2])? as usize)
                } else {
                    return Err("SBLKW requires [drive,] storage_addr, ram_src, count".to_string());
                };
                self.storage.block_write(drive, s_addr, &self.ram, r_src, count)
            }
            Opcode::SCOUNT => {
                let count = self.storage.drive_count() as i32;
                self.set_destination(&ci.raw_args, count)
            }
            Opcode::SSEL => {
                let drive = self.resolve_value(&ci.raw_args)? as usize;
                self.storage.select_drive(drive)
            }
            Opcode::SDRV => {
                let drive = self.storage.selected_drive() as i32;
                self.set_destination(&ci.raw_args, drive)
            }
            Opcode::SEXEC => {
                let drive = self.resolve_value(&ci.raw_args)? as usize;
                if let Some(prog) = self.storage.get_drive_program(drive).cloned() {
                    self.load_program(&prog);
                    Ok(())
                } else {
                    Err(format!("No executable program found on drive {}", drive))
                }
            }
            Opcode::FBMODE => {
                let mode = self.resolve_value(&ci.raw_args)?;
                self.framebuffer.set_mode(mode);
                Ok(())
            }
            Opcode::FBSET => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                if parts.len() != 3 { return Err("FBSET requires x, y, color".to_string()); }
                let x = self.resolve_value(&parts[0])?;
                let y = self.resolve_value(&parts[1])?;
                let color = self.resolve_value(&parts[2])? as u32;
                self.framebuffer.set_pixel(x, y, color);
                Ok(())
            }
            Opcode::FBGET => {
                let parts = Assembler::split_args(&ci.raw_args, 3);
                if parts.len() != 3 { return Err("FBGET requires dest, x, y".to_string()); }
                let x = self.resolve_value(&parts[1])?;
                let y = self.resolve_value(&parts[2])?;
                let color = self.framebuffer.get_pixel(x, y) as i32;
                self.set_destination(&parts[0], color)
            }
            Opcode::FBCLR => {
                let color = if !ci.raw_args.trim().is_empty() {
                    self.resolve_value(&ci.raw_args)? as u32
                } else {
                    0xFF051009
                };
                self.framebuffer.clear(color);
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
                self.framebuffer.fill_rect(x, y, w, h, color);
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
                self.framebuffer.draw_line(x0, y0, x1, y1, color);
                Ok(())
            }
            Opcode::FBLIT => {
                let parts = Assembler::split_args(&ci.raw_args, 5);
                if parts.len() != 5 { return Err("FBLIT requires x, y, w, h, ram_src".to_string()); }
                let x = self.resolve_value(&parts[0])?;
                let y = self.resolve_value(&parts[1])?;
                let w = self.resolve_value(&parts[2])?;
                let h = self.resolve_value(&parts[3])?;
                let ram_src = self.resolve_value(&parts[4])? as usize;
                self.framebuffer.blit(x, y, w, h, &self.ram, ram_src);
                Ok(())
            }
            Opcode::FBSWAP => {
                self.framebuffer.set_mode(1);
                self.framebuffer.set_dirty(true);
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
                let ram_src = self.resolve_value(&parts[0])? as usize;
                let count = self.resolve_value(&parts[1])? as usize;
                self.audio.write_buffer(&self.ram, ram_src, count);
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

        if (val_part.starts_with('\"') && val_part.ends_with('\"'))
            || (val_part.starts_with('\'') && val_part.ends_with('\''))
        {
            let text = &val_part[1..val_part.len() - 1];
            self.line_buffer.push_str(text);
        } else {
            let val = self.resolve_value(val_part)?;
            self.line_buffer.push_str(&val.to_string());
        }

        if newline {
            self.flush_line_buffer();
        }
        Ok(())
    }

    pub fn flush_line_buffer(&mut self) {
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
