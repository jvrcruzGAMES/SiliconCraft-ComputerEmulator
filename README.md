# SiliconCraft Virtual CPU Emulator (Rust)

A high-performance, cycle-accurate standalone emulator and toolchain for the **SiliconCraft 16/32-bit Virtual CPU architecture**, implemented in pure Rust.

Located at: `/home/jvrcruz/Documentos/Projects/SiliconCraft-ComputerEmulator`

---

## Features

- **All 71 Instructions Supported**:
  - Arithmetic & Logic: `MOV`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `INC`, `DEC`, `CMP`, `AND`, `OR`, `XOR`, `NOT`, `SHL`, `SHR`, `NEG`
  - Flow & Dynamic Branches: `JMP`, `JZ`, `JNZ`, `JS`, `JNS`, `JL`, `JLE`, `JG`, `JGE`, `CALL`, `RET`, dynamic register jumping (`JMP reg`, `CALL reg`)
  - Stack Operations: `PUSH`, `POP`, `PUSHA`, `POPA`
  - Interrupts & System: `IVEC`, `IRET`, `CLI`, `STI`, `INT`, `TIMER` (configurable 1-60 Hz hardware periodic timer), `VBLANK`
  - Storage Subsystem: `SSIZE`, `SREAD`, `SWRITE`, `SBLKR`, `SBLKW`, `SCOUNT`, `SSEL`, `SDRV`, `SEXEC` (up to 4 drives, 1024 MB each)
  - Color Framebuffer (240x135): `FBMODE`, `FBSET`, `FBGET`, `FBCLR`, `FBRECT`, `FBLINE`, `FBLIT`, `FBSWAP`, `FBWIDTH`, `FBHEIGHT`
  - Audio Synthesizer: `APUSH`, `ABUFW`, `ACLR`, `ASTAT`, `ATONE`, `APLAY` (8000 Hz 8-bit unsigned PCM)
  - I/O & Timers: `OUT`, `OUTH`, `OUTC`, `CLS`, `SLEEP`, `TIME`, `RAND`, `INPAD`
- **Static Assembler Directives**:
  - `ORG <address>`: Align origin in RAM
  - `DB <byte, ...>`: Define raw bytes or character strings
  - `DW <word, ...>`: Define 16-bit words
  - `DL <dword, ...>`: Define 32-bit dwords
  - `INCBIN "<path>"`: Include external binary files into RAM
- **Integrated Debugger & Visualizer**:
  - Interactive single-stepping (`--step`) with live opcode disassembly and register inspection
  - ANSI ASCII terminal display preview of the 240x135 framebuffer (`--dump-fb`)
  - Framebuffer export to PPM format (`--save-fb <output.ppm>`)
  - Audio output export to standard 8 kHz RIFF WAV file (`--save-audio <output.wav>`)
  - Full CPU register and memory dumping (`--dump-regs`, `--dump-ram <start> <words>`)
- **Zero External Dependencies**:
  - Pure standard library Rust (`std::*`) ensuring instant build and 100% portability.

---

## Building

```bash
# Debug build & test
cargo test
cargo build

# Release build (optimized)
cargo build --release
```

Binary is output to `target/release/siliconcraft-emulator`.

---

## Usage

```bash
siliconcraft-emulator [OPTIONS] <file.o | file.asm>
```

### Options:
- `--demo <name>`: Run one of the built-in demonstrations (`counter`, `fibonacci`, `jingle_bells`, `graphics`, `timer`)
- `--list-demos`: List all available built-in demos
- `--step`: Run in interactive single-step debugger mode (press Enter to advance, `q` to quit, `c` to continue)
- `--cycles <N>`: Maximum execution cycle limit (default: 10,000,000)
- `--dump-regs`: Print CPU registers (A, B, C, D, PC, FLAGS, Stacks) upon completion
- `--dump-ram <start> <count>`: Print memory slice starting at `<start>` for `<count>` words
- `--dump-fb`: Print ASCII preview of the 240x135 display in terminal
- `--save-fb <output.ppm>`: Save display raster to a PPM image
- `--save-audio <output.wav>`: Save audio buffer to an 8 kHz 8-bit WAV file

---

## Examples

### 1. Run Fibonacci Demo with Register Dump
```bash
cargo run -- --demo fibonacci --dump-regs
```

### 2. Run Graphics Demo and Render to Terminal
```bash
cargo run -- --demo graphics --dump-fb
```

### 3. Generate Audio and Export WAV
```bash
cargo run -- --demo jingle_bells --save-audio jingle.wav
```

### 4. Interactive Step-by-Step Debugging
```bash
cargo run -- --demo counter --step
```

### 5. Running Assembled Files
```bash
cargo run -- my_program.o --dump-regs
```
