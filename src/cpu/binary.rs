use super::instruction::Opcode;
use std::collections::HashMap;

// Stable SCBC v1 opcode IDs. Empty entries are reserved.
const OPCODES: &[&str] = &[
    "",          // 0
    "NOP",       // 1
    "MOV",       // 2
    "ADD",       // 3
    "SUB",       // 4
    "MUL",       // 5
    "DIV",       // 6
    "MOD",       // 7
    "INC",       // 8
    "DEC",       // 9
    "CMP",       // 10
    "AND",       // 11
    "OR",        // 12
    "XOR",       // 13
    "NOT",       // 14
    "SHL",       // 15
    "SHR",       // 16
    "JMP",       // 17
    "JZ",        // 18
    "JNZ",       // 19
    "JS",        // 20
    "JNS",       // 21
    "JL",        // 22
    "JLE",       // 23
    "JG",        // 24
    "JGE",       // 25
    "CALL",      // 26
    "RET",       // 27
    "PUSH",      // 28
    "POP",       // 29
    "IVEC",      // 30
    "IRET",      // 31
    "CLI",       // 32
    "STI",       // 33
    "INT",       // 34
    "OUT",       // 35
    "OUTH",      // 36
    "OUTC",      // 37
    "CLS",       // 38
    "SLEEP",     // 39
    "TIME",      // 40
    "RAND",      // 41
    "INPAD",     // 42
    "TIMER",     // 43
    "VBLANK",    // 44
    "SSIZE",     // 45
    "SREAD",     // 46
    "SWRITE",    // 47
    "SBLKR",     // 48
    "SBLKW",     // 49
    "SCOUNT",    // 50
    "SSEL",      // 51
    "SDRV",      // 52
    "SEXEC",     // 53
    "FBMODE",    // 54
    "FBSET",     // 55
    "FBGET",     // 56
    "FBCLR",     // 57
    "FBRECT",    // 58
    "FBLINE",    // 59
    "FBLIT",     // 60
    "FBSWAP",    // 61
    "FBWIDTH",   // 62
    "FBHEIGHT",  // 63
    "APUSH",     // 64
    "ABUFW",     // 65
    "ACLR",      // 66
    "ASTAT",     // 67
    "ATONE",     // 68
    "APLAY",     // 69
    "HLT",       // 70
    "",          // 71
    "PINFO",     // 72
    "PWRAP",     // 73
    "PCALL",     // 74
    "PSTATUS",   // 75
    "PRESULT",   // 76
    "PCALLS",    // 77
    "PSCAN",     // 78
    "NOPEN",     // 79
    "NSEND",     // 80
    "NRECV",     // 81
    "NCLOSE",    // 82
    "NHTTP",     // 83
    "NSTATUS",   // 84
    "NLEN",      // 85
    "NRESULT",   // 86
    "GALLOC",    // 87
    "NWSOPEN",   // 88
    "NWSTEXT",   // 89
    "NTLS",      // 90
    "NHTTP3",    // 91
    "",          // 92
    "NLISTEN",   // 93
    "NACCEPT",   // 94
    "NRESPOND",  // 95
    "NREADY",    // 96
    "NUTF8",     // 97
    "NRESPONDT", // 98
    "PLEN",      // 99
    "PRESULTS",  // 100
    "INAXIS",    // 101
    "GFXSEL",    // 102
    "GFXSTAT",   // 103
    "GPUCLR",    // 104
    "GPURECT",   // 105
    "GPULINE",   // 106
    "GPUCIRC",   // 107
    "GPUTRI",    // 108
    "GPUVRAM",   // 109
    "GPUBLIT",   // 110
    "GPUSWAP",   // 111
    "GPUSYNC",   // 112
];

pub fn has_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SCPK") || bytes.starts_with(b"SCBC")
}

pub(super) struct Image {
    pub outputs: HashMap<usize, Output>,
    pub segments: Vec<(i32, Vec<u8>)>,
}

#[derive(Clone)]
pub(super) struct Output {
    pub literal: Option<std::sync::Arc<str>>,
    pub newline: bool,
}

impl Image {
    pub fn decode(bytes: &[u8]) -> Result<(Vec<String>, Self), String> {
        let bytes = if bytes.starts_with(b"SCPK") {
            program_section(bytes)?
        } else {
            bytes
        };
        let mut input = Reader(bytes);
        if input.take(4)? != b"SCBC" {
            return Err("Expected an SCPK package or SCBC bytecode image".into());
        }
        if input.u16()? != 1 {
            return Err("Unsupported SCBC version".into());
        }
        input.take(2)?;
        let code_len = input.count(8)?;
        let code = input.take(code_len * 8)?;
        let strings = (0..input.count(4)?)
            .map(|_| {
                let len = input.u32()? as usize;
                std::str::from_utf8(input.take(len)?)
                    .map(std::sync::Arc::<str>::from)
                    .map_err(|_| "Invalid SCBC string encoding".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let ext_len = input.count(8)?;
        let extended = input.take(ext_len * 8)?;
        let segments = (0..input.count(8)?)
            .map(|_| {
                let address = input.u32()? as i32;
                let len = input.u32()? as usize;
                Ok((address, input.take(len)?.to_vec()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if code_len == 0 {
            return Err("SCBC program contains no instructions".into());
        }

        let mut lines = Vec::with_capacity(code_len);
        let mut image = Self {
            outputs: HashMap::new(),
            segments,
        };
        for (pc, raw) in code.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes(raw.try_into().unwrap());
            let (line, output) = decode_instruction(word, &strings, extended)
                .map_err(|error| format!("SCBC instruction {pc}: {error}"))?;
            lines.push(line);
            if let Some(output) = output {
                image.outputs.insert(pc, output);
            }
        }
        Ok((lines, image))
    }
}

fn program_section(bytes: &[u8]) -> Result<&[u8], String> {
    let mut input = Reader(bytes);
    input.take(4)?;
    if input.u16()? != 3 {
        return Err("Unsupported SCPK version".into());
    }
    input.take(2)?;
    let count = input.u16()? as usize;
    input.take(2)?;
    if !(2..=3).contains(&count) {
        return Err("Invalid SCPK section count".into());
    }
    let header = 12 + count * 12;
    let mut sections = [None, None, None];
    for index in 0..count {
        let id = input.u16()? as usize;
        input.take(2)?;
        let start = input.u32()? as usize;
        let size = input.u32()? as usize;
        let end = start
            .checked_add(size)
            .filter(|&end| end <= bytes.len())
            .ok_or("SCPK section out of bounds")?;
        if id == 0
            || id > sections.len()
            || sections[id - 1].is_some()
            || (index == 0 && (id != 1 || start != 0 || size != header))
            || (index != 0 && (start < header || size == 0))
            || sections
                .iter()
                .flatten()
                .any(|&(a, b)| start < b && a < end)
        {
            return Err("Invalid or overlapping SCPK sections".into());
        }
        sections[id - 1] = Some((start, end));
    }
    let (start, end) = sections[2].ok_or("SCPK is missing its program section")?;
    Ok(&bytes[start..end])
}

fn decode_instruction(
    word: u64,
    strings: &[std::sync::Arc<str>],
    extended: &[u8],
) -> Result<(String, Option<Output>), String> {
    let id = (word >> 56) as usize;
    let name = OPCODES
        .get(id)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("Unknown opcode ID {id}"))?;
    let opcode = Opcode::from_str(name);
    let format = (word >> 52) & 15;
    let flags = ((word >> 48) & 15) as usize;
    let immediate = word as i32;
    let mut args = Vec::new();
    let mut output =
        matches!(opcode, Opcode::OUT | Opcode::OUTH | Opcode::OUTC).then_some(Output {
            literal: None,
            newline: false,
        });
    match format {
        0 if flags <= 2 => {
            for (mode_shift, reg_shift) in [(45, 42), (39, 36)].into_iter().take(flags) {
                args.push(operand(
                    (word >> mode_shift) as u8 & 7,
                    ((word >> reg_shift) & 7) as i32,
                    immediate,
                )?);
            }
        }
        1 => {
            for (mode, bits) in [(flags >> 2, word >> 24), (flags & 3, word)] {
                let value = ((bits as i32) << 8) >> 8;
                args.push(operand([2, 3, 1, 4][mode], value, value)?);
            }
        }
        2 => {
            let output = output
                .as_mut()
                .ok_or("String operand is invalid for this opcode")?;
            output.literal = Some(
                strings
                    .get(immediate as usize)
                    .ok_or("SCBC string index out of bounds")?
                    .clone(),
            );
        }
        3 => {
            let offset = (word as u32 as usize)
                .checked_mul(8)
                .ok_or("SCBC operand index out of bounds")?;
            let end = offset
                .checked_add(flags * 8)
                .ok_or("SCBC operand index out of bounds")?;
            let operands = extended
                .get(offset..end)
                .ok_or("SCBC operand index out of bounds")?;
            for raw in operands.chunks_exact(8) {
                args.push(operand(
                    raw[0],
                    i32::from(raw[1]),
                    i32::from_le_bytes(raw[4..8].try_into().unwrap()),
                )?);
            }
        }
        _ => return Err(format!("Invalid instruction format {format}")),
    }
    if let Some(output) = &mut output {
        output.newline = match format {
            0 | 3 => word & (1 << 32) != 0,
            2 => flags & 1 != 0,
            _ => false,
        };
    }
    let line = if args.is_empty() {
        name.to_string()
    } else {
        format!("{name} {}", args.join(", "))
    };
    Ok((line, output))
}

fn operand(mode: u8, register: i32, immediate: i32) -> Result<String, String> {
    let reg = || {
        ["A", "B", "C", "D", "PC", "FLAGS"]
            .get(register as usize)
            .copied()
            .ok_or("Invalid SCBC register")
    };
    Ok(match mode {
        1 => reg()?.to_string(),
        2 => immediate.to_string(),
        3 => format!("[{immediate}]"),
        4 => format!("[{}]", reg()?),
        5 => format!("[{}+({immediate})]", reg()?),
        _ => return Err(format!("Invalid SCBC operand mode {mode}")),
    })
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let (value, rest) = self
            .0
            .split_at_checked(len)
            .ok_or("Truncated SiliconCraft binary")?;
        self.0 = rest;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn count(&mut self, min_width: usize) -> Result<usize, String> {
        let count = self.u32()? as usize;
        if count > 16_777_216 || count > self.0.len() / min_width {
            return Err("Invalid SCBC section count".into());
        }
        Ok(count)
    }
}
