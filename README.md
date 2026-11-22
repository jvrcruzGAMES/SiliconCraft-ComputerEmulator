# SiliconCraft Virtual CPU Emulator (Rust)

A high-performance, cycle-accurate standalone emulator and toolchain for the **SiliconCraft 16/32-bit Virtual CPU architecture**, implemented in pure Rust with a hardware-accelerated **Vulkan window** interface.

Located at: `/home/jvrcruz/Documentos/Projects/SiliconCraft-ComputerEmulator`

---

## Features

- **Hardware-Accelerated Vulkan Window**:
  - Direct rendering via Vulkan backend (`pixels` + `wgpu` + `winit`).
  - Resizable window (960 × 540 default, 4× scaling of native 240 × 135 display) with 16:9 aspect ratio preservation.
  - **Text Console Mode (`FBMODE 0`)**:
    - Replicates the in-game CRT monitor with phosphor green/dark background (`0xFF051009`) and subtle raster scanlines.
    - Crisp 8×8 bitmap font (`font8x8`) displaying 60 columns × 27 lines with automatic line wrapping.
    - Mouse wheel and Page Up / Page Down scrollback history.
    - Color-coded telemetry: Phosphor Green (stdout), Cyan (BIOS/OK), Red (CPU exceptions/errors), Amber (warnings).
    - Blinking hardware cursor.
    - **F2**: Toggle between Standard (60×27) and Large (30×15) font scale.
  - **Framebuffer Mode (`FBMODE 1` / `FBSWAP`)**:
    - 240 × 135 direct 32-bit ARGB pixel canvas rendered with pixel-perfect 2×2 integer scaling into the GPU buffer.
    - Instant hardware flipping and drawing primitives (`FBSET`, `FBRECT`, `FBLINE`, `FBLIT`, `FBCLR`).
  - **Interactive Keyboard Input**:
    - Keystrokes are captured and routed directly to the virtual CPU's keyboard interrupt queue (`VEC_KEYBOARD`, vector 10).
    - **F5**: Instant CPU reset and reboot.
    - **F10**: Pause / Resume execution.
- **Hardware Specifications (Default Preset)**:
  - **4 Sticks of RAM**: 4096 KB (1,048,576 words).
  - **NVMe Storage in Extension Slot 1**: 1024 MB non-volatile drive.
  - **Code Burned to ROM**: Non-volatile program memory preserved across CPU resets.
  - Pure machine behavior (no intrusive host banners on startup).
- **All 71 Virtual CPU Instructions Supported**:
  - Arithmetic & Logic: `MOV`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `INC`, `DEC`, `CMP`, `AND`, `OR`, `XOR`, `NOT`, `SHL`, `SHR`, `NEG`
  - Flow & Dynamic Branches: `JMP`, `JZ`, `JNZ`, `JS`, `JNS`, `JL`, `JLE`, `JG`, `JGE`, `CALL`, `RET`, dynamic register jumping (`JMP reg`, `CALL reg`)
  - Stack Operations: `PUSH`, `POP`, `PUSHA`, `POPA`
  - Interrupts & System: `IVEC`, `IRET`, `CLI`, `STI`, `INT`, `TIMER` (configurable 1-60 Hz hardware periodic timer), `VBLANK`
  - Storage Subsystem: `SSIZE`, `SREAD`, `SWRITE`, `SBLKR`, `SBLKW`, `SCOUNT`, `SSEL`, `SDRV`, `SEXEC` (up to 4 drives, 1024 MB each)
  - Color Framebuffer (240x135): `FBMODE`, `FBSET`, `FBGET`, `FBCLR`, `FBRECT`, `FBLINE`, `FBLIT`, `FBSWAP`, `FBWIDTH`, `FBHEIGHT`
  - Audio Synthesizer: `APUSH`, `ABUFW`, `ACLR`, `ASTAT`, `ATONE`, `APLAY` (8000 Hz 8-bit unsigned PCM)
  - I/O & Timers: `OUT`, `OUTH`, `OUTC`, `CLS`, `SLEEP`, `TIME`, `RAND`, `INPAD`
- **Integrated High-Level Toolchain & Multi-File Project Resolver**:
  - **N Compiler (`NCompiler`)**: Compiles `.n` source files, functions, variables, expressions, and inline `asm { ... }` blocks.
  - **Assembly Resolver (`ProjectResolver`)**:
    - Supports `.n`, `.asm`, `.s`, and `.S` files in mixed or pure projects.
    - Resolves `import`, `.import`, `include`, `.include`, `#include` directives.
    - Bidirectional interop: `.n` can import `.asm` subroutines, `.asm` can import `.asm` or `.n`.
    - Automatically wraps imported assembly modules with `JMP _import_skip_X` to prevent execution fallthrough on machine startup.
    - Produces a single assembled bytecode `.o` instruction stream.
- **Static Assembler Directives**:
  - `ORG <address>`: Align origin in RAM
  - `DB <byte, ...>`: Define raw bytes or character strings
  - `DW <word, ...>`: Define 16-bit words
  - `DL <dword, ...>`: Define 32-bit dwords
  - `INCBIN "<path>"`: Include external binary files into RAM
- **Diagnostic & CLI Tools**:
  - Interactive single-stepping (`--step`) in terminal
  - Headless execution mode (`--headless`) for batch scripts and automated test environments
  - ANSI ASCII terminal display preview of the 240x135 framebuffer (`--dump-fb`)
  - Framebuffer export to PPM format (`--save-fb <output.ppm>`)
  - Audio output export to standard 8 kHz RIFF WAV file (`--save-audio <output.wav>`)
  - Full CPU register and memory dumping (`--dump-regs`, `--dump-ram <start> <words>`)

---

## Building

```bash
# Debug build & run tests
cargo test
cargo build

# Release build (optimized)
cargo build --release
```

Binary is output to `target/release/siliconcraft-emulator`.

---

## Usage

```bash
siliconcraft-emulator [OPTIONS] <file.o | file.asm | file.n | project_dir>
```

### Options:
- `--demo <name>`: Run one of the built-in demonstrations (`counter`, `fibonacci`, `jingle_bells`, `graphics`, `timer`)
- `--list-demos`: List all available built-in demos
- `--headless`: Run in headless terminal mode without opening the Vulkan window
- `--step`: Run in interactive single-step debugger mode in terminal
- `--cycles <N>`: Maximum execution cycle limit (default: 10,000,000)
- `--dump-regs`: Print CPU registers (A, B, C, D, PC, FLAGS, Stacks) upon completion
- `--dump-ram <start> <count>`: Print memory slice starting at `<start>` for `<count>` words
- `--dump-fb`: Print ASCII preview of the 240x135 display in terminal
- `--save-fb <output.ppm>`: Save display raster to a PPM image
- `--save-audio <output.wav>`: Save audio buffer to an 8 kHz 8-bit WAV file

### Window Hotkeys:
- **F5**: Reset CPU, clear registers, and reboot from line 0
- **F2**: Toggle between Standard (60×27) and Large (30×15) font scale
- **F10**: Pause / Resume CPU execution
- **Mouse Wheel / Page Up / Page Down**: Scroll terminal text history

---

## Examples

### 1. Launch Interactive Vulkan Window with Graphics Demo
```bash
siliconcraft-emulator --demo graphics
```

### 2. Run a Multi-File N + Assembly Project
```bash
siliconcraft-emulator path/to/my_project/
```

### 3. Run Fibonacci Demo in Headless Terminal Mode with Register Dump
```bash
siliconcraft-emulator --demo fibonacci --headless --dump-regs
```

### 4. Interactive Step-by-Step Debugging in Terminal
```bash
siliconcraft-emulator --demo counter --step
```
