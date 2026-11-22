#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    PC = 4,
    FLAGS = 5,
}

impl Register {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "A" => Some(Register::A),
            "B" => Some(Register::B),
            "C" => Some(Register::C),
            "D" => Some(Register::D),
            "PC" => Some(Register::PC),
            "FLAGS" => Some(Register::FLAGS),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    // Flow & Control
    NOP,
    HLT,
    JMP,
    JZ,
    JNZ,
    JS,
    JNS,
    JL,
    JLE,
    JG,
    JGE,
    CALL,
    RET,
    // Data movement & Stack
    MOV,
    PUSH,
    POP,
    PUSHA,
    POPA,
    // Arithmetic & Logic
    ADD,
    SUB,
    MUL,
    DIV,
    MOD,
    INC,
    DEC,
    CMP,
    AND,
    OR,
    XOR,
    NOT,
    SHL,
    SHR,
    // I/O & System
    OUT,
    OUTC,
    OUTH,
    CLS,
    SLEEP,
    TIME,
    RAND,
    INPAD,
    // Storage
    SSIZE,
    SREAD,
    SWRITE,
    SBLKR,
    SBLKW,
    SCOUNT,
    SSEL,
    SDRV,
    SEXEC,
    // Framebuffer
    FBMODE,
    FBSET,
    FBGET,
    FBCLR,
    FBRECT,
    FBLINE,
    FBLIT,
    FBSWAP,
    FBWIDTH,
    FBHEIGHT,
    // Audio
    APUSH,
    ABUFW,
    ACLR,
    ASTAT,
    ATONE,
    APLAY,
    // Interrupts
    IVEC,
    IRET,
    CLI,
    STI,
    INT,
    TIMER,
    VBLANK,
    // Directives
    ORG,
    DB,
    DW,
    DL,
    INCBIN,
    UNKNOWN,
}

impl Opcode {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "NOP" => Opcode::NOP,
            "HLT" => Opcode::HLT,
            "JMP" => Opcode::JMP,
            "JZ" => Opcode::JZ,
            "JNZ" => Opcode::JNZ,
            "JS" => Opcode::JS,
            "JNS" => Opcode::JNS,
            "JL" => Opcode::JL,
            "JLE" => Opcode::JLE,
            "JG" => Opcode::JG,
            "JGE" => Opcode::JGE,
            "CALL" => Opcode::CALL,
            "RET" => Opcode::RET,
            "MOV" => Opcode::MOV,
            "PUSH" => Opcode::PUSH,
            "POP" => Opcode::POP,
            "PUSHA" => Opcode::PUSHA,
            "POPA" => Opcode::POPA,
            "ADD" => Opcode::ADD,
            "SUB" => Opcode::SUB,
            "MUL" => Opcode::MUL,
            "DIV" => Opcode::DIV,
            "MOD" => Opcode::MOD,
            "INC" => Opcode::INC,
            "DEC" => Opcode::DEC,
            "CMP" => Opcode::CMP,
            "AND" => Opcode::AND,
            "OR" => Opcode::OR,
            "XOR" => Opcode::XOR,
            "NOT" => Opcode::NOT,
            "SHL" => Opcode::SHL,
            "SHR" => Opcode::SHR,
            "OUT" => Opcode::OUT,
            "OUTC" => Opcode::OUTC,
            "OUTH" => Opcode::OUTH,
            "CLS" => Opcode::CLS,
            "SLEEP" | "PAUSE" => Opcode::SLEEP,
            "TIME" => Opcode::TIME,
            "RAND" => Opcode::RAND,
            "INPAD" => Opcode::INPAD,
            "SSIZE" => Opcode::SSIZE,
            "SREAD" => Opcode::SREAD,
            "SWRITE" => Opcode::SWRITE,
            "SBLKR" => Opcode::SBLKR,
            "SBLKW" => Opcode::SBLKW,
            "SCOUNT" => Opcode::SCOUNT,
            "SSEL" => Opcode::SSEL,
            "SDRV" => Opcode::SDRV,
            "SEXEC" | "SBOOT" => Opcode::SEXEC,
            "FBMODE" | "DMODE" => Opcode::FBMODE,
            "FBSET" | "PIX" => Opcode::FBSET,
            "FBGET" => Opcode::FBGET,
            "FBCLR" | "FCLR" => Opcode::FBCLR,
            "FBRECT" | "RECT" => Opcode::FBRECT,
            "FBLINE" | "LINE" => Opcode::FBLINE,
            "FBLIT" => Opcode::FBLIT,
            "FBSWAP" => Opcode::FBSWAP,
            "FBWIDTH" => Opcode::FBWIDTH,
            "FBHEIGHT" => Opcode::FBHEIGHT,
            "APUSH" => Opcode::APUSH,
            "ABUFW" => Opcode::ABUFW,
            "ACLR" => Opcode::ACLR,
            "ASTAT" => Opcode::ASTAT,
            "ATONE" | "TONE" => Opcode::ATONE,
            "APLAY" => Opcode::APLAY,
            "IVEC" => Opcode::IVEC,
            "IRET" => Opcode::IRET,
            "CLI" => Opcode::CLI,
            "STI" => Opcode::STI,
            "INT" => Opcode::INT,
            "TIMER" | "SETTIMER" => Opcode::TIMER,
            "VBLANK" => Opcode::VBLANK,
            "ORG" => Opcode::ORG,
            "DB" => Opcode::DB,
            "DW" => Opcode::DW,
            "DL" => Opcode::DL,
            "INCBIN" => Opcode::INCBIN,
            _ => Opcode::UNKNOWN,
        }
    }

    pub fn is_directive(&self) -> bool {
        matches!(self, Opcode::ORG | Opcode::DB | Opcode::DW | Opcode::DL | Opcode::INCBIN)
    }

    pub fn is_jump(&self) -> bool {
        matches!(
            self,
            Opcode::JMP
                | Opcode::JZ
                | Opcode::JNZ
                | Opcode::JS
                | Opcode::JNS
                | Opcode::JL
                | Opcode::JLE
                | Opcode::JG
                | Opcode::JGE
                | Opcode::CALL
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    None,
    Reg(Register),
    Imm(i32),
    RamImm(usize),
    RamReg(Register),
    Dynamic(String),
}

#[derive(Debug, Clone)]
pub struct CompiledInstruction {
    pub opcode: Opcode,
    pub dest: Operand,
    pub src: Operand,
    pub target_pc: Option<usize>,
    pub raw_line: String,
    pub raw_args: String,
}
