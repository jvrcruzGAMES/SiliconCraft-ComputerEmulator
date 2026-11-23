# SiliconCraft Virtual CPU Emulator (Rust)

A high-performance, cycle-accurate standalone emulator and toolchain for the **SiliconCraft 16/32-bit Virtual CPU architecture**, with a **Vulkan window** and SDL3 for desktop controller input and audio.

---

## Features

- **Hardware-Accelerated Vulkan Window**:
  - Direct rendering via Vulkan backend (`pixels` + `wgpu` + `winit`).
  - Resizable window (960 × 540 default) with 16:9 aspect ratio preservation.
  - **Text Console Mode (`FBMODE 0`)**:
    - Replicates the in-game CRT monitor with phosphor green/dark background (`0xFF051009`) and subtle raster scanlines.
    - Crisp 8×8 bitmap font (`font8x8`) displaying 117 columns × 51 lines with automatic line wrapping.
    - Mouse wheel and Page Up / Page Down scrollback history.
    - Color-coded telemetry: Phosphor Green (stdout), Cyan (BIOS/OK), Red (CPU exceptions/errors), Amber (warnings).
    - Blinking hardware cursor.
    - **F2**: Toggle between standard and large font scale.
  - **Framebuffer Mode (`FBMODE 1` / `FBSWAP`)**:
    - 960 × 540 32-bit ARGB framebuffer, matching the SiliconCraft computer.
    - Instant hardware flipping and drawing primitives (`FBSET`, `FBRECT`, `FBLINE`, `FBLIT`, `FBCLR`).
  - **Interactive Keyboard Input**:
    - Keystrokes are captured and routed directly to the virtual CPU's keyboard interrupt queue (`VEC_KEYBOARD`, vector 10).
    - The first SDL3 controller shares `INPAD` port 0 with the keyboard. Additional controllers occupy ports 1–3. Buttons, left stick, D-pad and triggers use the SiliconCraft masks.
    - `INPAD dest, -1` consumes a queued keyboard character and `INPAD dest, -2` peeks at it. Both return `-1` if the queue is empty.
    - Mouse movement and clicks report coordinates in the 960 × 540 framebuffer.
    - **F5**: Instant CPU reset and reboot.
    - **F10**: Pause / Resume execution.
- **Hardware Specifications (Default Preset)**:
  - **RAM** has eight 2 MiB modules for a total of 16 MiB (4,194,304 words).
  - **Storage** has 2 GiB of onboard flash and two 2 GiB NVMe expansion drives. Drives use sparse memory by default. Use `--drive` for a sparse image that persists across emulator sessions.
  - **Code Burned to ROM**: Non-volatile program memory preserved across CPU resets.
  - Pure machine behavior (no intrusive host banners on startup).
- **SiliconCraft CPU Instructions**:
  - Arithmetic & Logic: `MOV`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `INC`, `DEC`, `CMP`, `AND`, `OR`, `XOR`, `NOT`, `SHL`, `SHR`
  - Flow & Dynamic Branches: `JMP`, `JZ`, `JNZ`, `JS`, `JNS`, `JL`, `JLE`, `JG`, `JGE`, `CALL`, `RET`, dynamic register jumping (`JMP reg`, `CALL reg`)
  - Stack Operations: `PUSH`, `POP`, `PUSHA`, `POPA`
  - Interrupts & System: `IVEC`, `IRET`, `CLI`, `STI`, `INT`, `TIMER` (configurable 1-60 Hz hardware periodic timer), `VBLANK`
  - Storage Subsystem: `SSIZE`, `SREAD`, `SWRITE`, `SBLKR`, `SBLKW`, `SCOUNT`, `SSEL`, `SDRV`, `SEXEC` (three installed drives at 2 GiB each)
  - Color Framebuffer (960x540): `FBMODE`, `FBSET`, `FBGET`, `FBCLR`, `FBRECT`, `FBLINE`, `FBLIT`, `FBSWAP`, `FBWIDTH`, `FBHEIGHT`
  - Peripherals: `PSCAN`, `PINFO`, `PWRAP`, `PCALL`, `PCALLS`, `PSTATUS`, `PRESULT`. Embedders can attach devices by address through the Rust `Peripheral` trait.
  - Networking: `NOPEN`, `NSEND`, `NRECV`, `NCLOSE`, `NHTTP`, `NSTATUS`, `NLEN`, `NRESULT`. TCP, UDP and HTTP(S) are available for explicitly enabled host and port pairs.
  - Audio Synthesizer: `APUSH`, `ABUFW`, `ACLR`, `ASTAT`, `ATONE`, `APLAY` (8000 Hz 8-bit unsigned PCM)
  - I/O & Timers: `OUT`, `OUTH`, `OUTC`, `CLS`, `SLEEP`, `TIME`, `RAND`, `INPAD`
- **Integrated High-Level Toolchain & Multi-File Project Resolver**:
  - **N2 Compiler** runs the compiler from the sibling SiliconCraft checkout and supports individual `.n2` files and N2 project directories.
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
  - ANSI ASCII terminal display preview of the 960x540 framebuffer (`--dump-fb`)
  - Framebuffer export to PPM format (`--save-fb <output.ppm>`)
  - Audio output export to standard 8 kHz RIFF WAV file (`--save-audio <output.wav>`)
  - Full CPU register and memory dumping (`--dump-regs`, `--dump-ram <start> <words>`)

---

## Building

The build needs Rust, a C/C++ toolchain and CMake to build the bundled SDL3. On Linux, install the development packages for your SDL audio backend (such as ALSA or PipeWire) before building to enable sound output. The Vulkan window needs a desktop session and a Vulkan driver.

N2 programs also need a Java 21 JDK with `java` and `javac` on your PATH or available under `JAVA_HOME`. Set `SILICONCRAFT_SOURCE` to the SiliconCraft checkout if it sits outside the default sibling directory.

```bash
# Debug build & run tests
cargo test --locked
cargo build --locked

# Release build (optimized)
cargo build --locked --release
```

Binary is output to `target/release/siliconcraft-emulator`.

---

## Usage

```bash
siliconcraft-emulator [OPTIONS] <file.o | file.asm | file.n | file.n2 | project_dir>
```

### Options:
- `--demo <name>`: Run one of the built-in demonstrations (`counter`, `fibonacci`, `jingle_bells`, `graphics`, `timer`)
- `--list-demos`: List all available built-in demos
- `--headless`: Run in headless terminal mode without opening the Vulkan window
- `--drive <slot:path>` attaches a persistent image to drive 0, 1 or 2. For example, `--drive 1:nvme.img`.
- `--allow-network <host:port>` enables guest networking for an exact endpoint. Repeat the flag for multiple endpoints.
- `--step`: Run in interactive single-step debugger mode in terminal
- `--cycles <N>`: Maximum execution cycle limit (default: 10,000,000)
- `--dump-regs`: Print CPU registers (A, B, C, D, PC, FLAGS, Stacks) upon completion
- `--dump-ram <start> <count>`: Print memory slice starting at `<start>` for `<count>` words
- `--dump-fb`: Print ASCII preview of the 960x540 display in terminal
- `--save-fb <output.ppm>`: Save display raster to a PPM image
- `--save-audio <output.wav>`: Save audio buffer to an 8 kHz 8-bit WAV file

At 2 GiB, `SSIZE` returns the 32-bit bit pattern `0x80000000`, which appears as `-2147483648` in a signed register. Byte addresses run from 0 through `0x7fffffff`.

`FBLIT` now uses the SiliconCraft operand order `FBLIT x, y, ram_src, width, height`.

### Window hotkeys
- **F5**: Reset CPU, clear registers, and reboot from line 0
- **F2**: Toggle between standard and large font scale
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

## Publishing a release

In GitHub, open **Actions → Release emulator → Run workflow**, select the
branch or ref to build, and enter a tag such as `v0.1.0`. Once both builds
pass, the workflow creates the tag at the built commit if needed. An existing
tag can be used if it points to that same commit and has no release yet.

You can also push a version tag from the commit you want to release. For example, use
`git tag v0.1.0` followed by `git push origin v0.1.0` for a new tag.

GitHub Actions tests and builds x86-64 binaries on Windows and Ubuntu 24.04,
then publishes a single GitHub release with both the Windows ZIP and Linux
tarball. The Linux binary targets Ubuntu 24.04 or a compatible distribution.
