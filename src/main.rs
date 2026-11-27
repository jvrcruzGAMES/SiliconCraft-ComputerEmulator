use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use siliconcraft_emulator::{demos, VirtualCpu, State, ProjectResolver, NCompiler, run_emulator_window};

enum ProgramInput {
    Source(Vec<String>),
    Binary(Vec<u8>),
}

fn print_help() {
    println!("SiliconCraft Virtual CPU Emulator");
    println!("Usage: siliconcraft-emulator [OPTIONS] <file.sc | file.o | file.asm | file.n | file.n2 | project_dir>");
    println!();
    println!("Options:");
    println!("  --demo <name>             Run a built-in demo ({})", demos::list_demos().join(", "));
    println!("  --list-demos              List available built-in demos");
    println!("  --headless                Run in headless terminal mode without opening the Vulkan window");
    println!("  --step                    Interactive single-step debugger mode");
    println!("  --cycles <N>              Limit execution to N cycles (default: 10,000,000)");
    println!("  --dump-regs               Print CPU registers and flags after execution");
    println!("  --dump-ram <start> <num>  Dump RAM contents from <start> for <num> words");
    println!("  --dump-fb                 Display ANSI ASCII view of the 960x540 framebuffer");
    println!("  --save-fb <output.ppm>    Save framebuffer image to a PPM file");
    println!("  --save-audio <output.wav> Save generated audio samples to an 8kHz 8-bit WAV file");
    println!("  --drive <slot:path>       Attach a persistent image to drive 0, 1, or 2");
    println!("  --allow-network <host:port> Permit guest networking to an exact endpoint (repeatable)");
    println!("  --allow-listen <port>       Permit guest listeners on a local port (repeatable)");
    println!("  --tls-cert <file.pem>       Certificate chain for guest HTTPS and WSS listeners");
    println!("  --tls-key <file.pem>        Private key for guest HTTPS and WSS listeners");
    println!("  --help, -h                Show this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        print_help();
        return;
    }

    let mut drive_images = Vec::new();
    let mut network_endpoints = Vec::new();
    let mut listen_ports: Vec<u16> = Vec::new();
    let mut tls_cert: Option<String> = None;
    let mut tls_key: Option<String> = None;
    let mut file_path: Option<String> = None;
    let mut demo_name: Option<String> = None;
    let mut step_mode = false;
    let mut headless_mode = false;
    let mut max_cycles: usize = 10_000_000;
    let mut dump_regs = false;
    let mut dump_ram: Option<(usize, usize)> = None;
    let mut dump_fb = false;
    let mut save_fb: Option<String> = None;
    let mut save_audio: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--version" | "-v" => {
                println!("SiliconCraft Virtual CPU Emulator v0.1.0");
                return;
            }
            "--list-demos" => {
                println!("Available Demos:");
                for d in demos::list_demos() {
                    if let Some((desc, _)) = demos::get_demo(d) {
                        println!("  - {:<15} {}", d, desc);
                    }
                }
                return;
            }
            "--demo" => {
                if i + 1 < args.len() {
                    demo_name = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("Error: --demo requires a demo name");
                    return;
                }
            }
            "--drive" => {
                let image = args.get(i + 1).and_then(|value| value.split_once(':'));
                let Some((slot, path)) = image.and_then(|(slot, path)| {
                    let slot = slot.parse::<usize>().ok()?;
                    (slot < 3 && !path.is_empty()).then_some((slot, path))
                }) else {
                    eprintln!("Error: --drive requires slot:path with slot 0, 1 or 2");
                    std::process::exit(2);
                };
                if drive_images.iter().any(|&(configured, _)| configured == slot) {
                    eprintln!("Error: drive {slot} was specified more than once");
                    std::process::exit(2);
                }
                drive_images.push((slot, path));
                i += 1;
            }
            "--allow-network" => {
                let endpoint = args.get(i + 1).and_then(|value| value.rsplit_once(':'));
                let Some((host, port)) = endpoint.and_then(|(host, port)| {
                    let port = port.parse::<u16>().ok()?;
                    (!host.is_empty() && port > 0).then_some((host, port))
                }) else {
                    eprintln!("Error: --allow-network requires host:port");
                    std::process::exit(2);
                };
                network_endpoints.push((host, port));
                i += 1;
            }
            "--allow-listen" => {
                let Some(port) = args.get(i + 1).and_then(|value| value.parse::<u16>().ok()).filter(|&p| p > 0) else {
                    eprintln!("Error: --allow-listen requires a port from 1 to 65535");
                    std::process::exit(2);
                };
                listen_ports.push(port);
                i += 1;
            }
            "--tls-cert" | "--tls-key" => {
                let Some(path) = args.get(i + 1) else {
                    eprintln!("Error: {} requires a file path", args[i]);
                    std::process::exit(2);
                };
                if args[i] == "--tls-cert" { tls_cert = Some(path.clone()) } else { tls_key = Some(path.clone()) }
                i += 1;
            }
            "--headless" => {
                headless_mode = true;
            }
            "--step" => {
                step_mode = true;
            }
            "--cycles" => {
                if i + 1 < args.len() {
                    max_cycles = args[i + 1].parse().unwrap_or(max_cycles);
                    i += 1;
                }
            }
            "--dump-regs" => {
                dump_regs = true;
            }
            "--dump-ram" => {
                if i + 2 < args.len() {
                    let start = args[i + 1].parse().unwrap_or(0);
                    let count = args[i + 2].parse().unwrap_or(64);
                    dump_ram = Some((start, count));
                    i += 2;
                }
            }
            "--dump-fb" => {
                dump_fb = true;
            }
            "--save-fb" => {
                if i + 1 < args.len() {
                    save_fb = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--save-audio" => {
                if i + 1 < args.len() {
                    save_audio = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            arg if !arg.starts_with('-') => {
                file_path = Some(arg.to_string());
            }
            unknown => {
                eprintln!("Unknown argument: {}", unknown);
                print_help();
                return;
            }
        }
        i += 1;
    }

    let program = if let Some(demo) = demo_name {
        if let Some((_, lines)) = demos::get_demo(&demo) {
            ProgramInput::Source(lines)
        } else {
            eprintln!("Unknown demo '{}'. Run with --list-demos to see available demos.", demo);
            return;
        }
    } else if let Some(path) = file_path {
        let p = Path::new(&path);
        let extension = p.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let is_package = extension.eq_ignore_ascii_case("sc");
        let binary = if !p.is_dir() && (is_package || extension.eq_ignore_ascii_case("o")) {
            match fs::read(p) {
                Ok(bytes) if is_package || siliconcraft_emulator::cpu::binary::has_magic(&bytes) => Some(bytes),
                Ok(_) => None,
                Err(error) => {
                    eprintln!("Failed to read {path}: {error}");
                    std::process::exit(1);
                }
            }
        } else {
            None
        };
        if let Some(bytes) = binary {
            ProgramInput::Binary(bytes)
        } else if p.is_dir() {
            match ProjectResolver::load_project_from_dir(p) {
                Ok(files) => match ProjectResolver::compile_project(&files, None) {
                    Ok(lines) => ProgramInput::Source(lines),
                    Err(e) => {
                        eprintln!("Compilation failed: {}", e);
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read project directory '{}': {}", path, e);
                    return;
                }
            }
        } else {
            let parent = p.parent().unwrap_or(Path::new("."));
            let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let parent_dir = if parent.as_os_str().is_empty() { Path::new(".") } else { parent };
            let mut resolved = None;
            if let Ok(files) = ProjectResolver::load_project_from_dir(parent_dir) {
                if files.contains_key(&filename) {
                    match ProjectResolver::compile_project(&files, Some(&filename)) {
                        Ok(lines) => resolved = Some(ProgramInput::Source(lines)),
                        Err(error) if ProjectResolver::is_n2_file(&filename) => {
                            eprintln!("Compilation failed: {error}");
                            std::process::exit(1);
                        }
                        Err(_) => {}
                    }
                }
            }
            if let Some(res) = resolved {
                res
            } else {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let raw_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                        if ProjectResolver::is_n2_file(&filename) {
                            let project = std::collections::HashMap::from([(filename.clone(), raw_lines)]);
                            match ProjectResolver::compile_project(&project, Some(&filename)) {
                                Ok(lines) => ProgramInput::Source(lines),
                                Err(error) => {
                                    eprintln!("Compilation failed: {error}");
                                    std::process::exit(1);
                                }
                            }
                        } else if path.ends_with(".n") {
                            match NCompiler::compile(&raw_lines) {
                                Ok(compiled) => ProgramInput::Source(compiled),
                                Err(err) => {
                                    eprintln!("Compilation failed: {}", err);
                                    return;
                                }
                            }
                        } else {
                            ProgramInput::Source(raw_lines)
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read file '{}': {}", path, e);
                        return;
                    }
                }
            }
        }
    } else {
        eprintln!("Error: No program file or demo specified.");
        print_help();
        return;
    };

    let mut cpu = VirtualCpu::default();
    for (slot, path) in drive_images {
        if let Err(error) = cpu.storage.attach_image(slot, Path::new(path)) {
            eprintln!("Could not attach drive {slot} from {path}: {error}");
            std::process::exit(1);
        }
    }
    for (host, port) in network_endpoints {
        cpu.network_port.allow(host, port);
    }
    for port in listen_ports {
        cpu.network_port.allow_listen(port);
    }
    match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            if let Err(error) = cpu.network_port.set_server_tls(Path::new(&cert), Path::new(&key)) {
                eprintln!("Could not load the TLS identity: {error}");
                std::process::exit(1);
            }
        }
        (None, None) => {}
        _ => {
            eprintln!("Error: --tls-cert and --tls-key must be used together");
            std::process::exit(2);
        }
    }
    match program {
        ProgramInput::Source(lines) => cpu.burn_to_rom(&lines),
        ProgramInput::Binary(bytes) => {
            if let Err(error) = cpu.load_binary(&bytes) {
                eprintln!("Could not load binary: {error}");
                std::process::exit(1);
            }
        }
    }

    if step_mode {
        run_interactive_debugger(&mut cpu);
    } else if headless_mode {
        let mut executed = 0;
        let batch_size = 5000;
        while cpu.state == State::Running && executed < max_cycles {
            cpu.tick(batch_size);
            executed += batch_size;

            // Print any new terminal lines
            while !cpu.terminal_output.is_empty() {
                let line = cpu.terminal_output.remove(0);
                println!("{}", line);
            }
        }
    } else {
        if let Err(e) = run_emulator_window(cpu) {
            eprintln!("Vulkan window error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // Flush remaining terminal output
    cpu.flush_line_buffer();
    while !cpu.terminal_output.is_empty() {
        let line = cpu.terminal_output.remove(0);
        println!("{}", line);
    }

    if let State::Error(ref err) = cpu.state {
        eprintln!("{}", err);
    }

    if dump_regs || step_mode {
        println!("\n=== Register Dump ===");
        println!("  A:     {:10} (0x{:08X})", cpu.reg_a, cpu.reg_a);
        println!("  B:     {:10} (0x{:08X})", cpu.reg_b, cpu.reg_b);
        println!("  C:     {:10} (0x{:08X})", cpu.reg_c, cpu.reg_c);
        println!("  D:     {:10} (0x{:08X})", cpu.reg_d, cpu.reg_d);
        println!("  PC:    {:10} (0x{:08X})", cpu.pc, cpu.pc);
        println!("  FLAGS: Z={} S={}", cpu.flag_z, cpu.flag_s);
        println!("  DataStack: {:?}", cpu.data_stack);
        println!("  CallStack: {:?}", cpu.call_stack);
    }

    if let Some((start, count)) = dump_ram {
        println!("\n=== RAM Dump [{}..{}] ===", start, start + count);
        print!("{}", cpu.ram.dump(start, count));
    }

    if dump_fb {
        println!("\n=== Framebuffer ASCII View (960x540) ===");
        print!("{}", cpu.active_framebuffer().render_ascii());
    }

    if let Some(ppm_path) = save_fb {
        match cpu.active_framebuffer().save_ppm(&ppm_path) {
            Ok(_) => println!("Framebuffer image saved to: {}", ppm_path),
            Err(e) => eprintln!("Failed to save framebuffer to '{}': {}", ppm_path, e),
        }
    }

    if let Some(wav_path) = save_audio {
        let samples = cpu.audio.recorded_samples();
        if samples.is_empty() {
            println!("No audio samples were generated during execution.");
        } else {
            match siliconcraft_emulator::AudioSubsystem::save_wav(&wav_path, samples) {
                Ok(_) => println!("Audio ({} samples, {} ms) saved to: {}", samples.len(), samples.len() * 1000 / 8000, wav_path),
                Err(e) => eprintln!("Failed to save audio to '{}': {}", wav_path, e),
            }
        }
    }
}

fn run_interactive_debugger(cpu: &mut VirtualCpu) {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    println!("[Interactive Debugger] Commands: (Enter)=Step, c=Continue, r=Registers, m <addr>=Memory, q=Quit");

    while cpu.state == State::Running || cpu.state == State::Paused {
        let current_line = if cpu.pc < cpu.program.len() {
            &cpu.program[cpu.pc]
        } else {
            "[END OF PROGRAM]"
        };

        print!("[PC:{:04}] {:<35} | A:{:<5} B:{:<5} C:{:<5} D:{:<5} Z:{} S:{} > ",
            cpu.pc, current_line.trim(), cpu.reg_a, cpu.reg_b, cpu.reg_c, cpu.reg_d,
            if cpu.flag_z { '1' } else { '0' },
            if cpu.flag_s { '1' } else { '0' },
        );
        io::stdout().flush().unwrap();

        line.clear();
        if reader.read_line(&mut line).is_err() {
            break;
        }
        let cmd = line.trim();

        match cmd {
            "q" | "quit" | "exit" => {
                println!("Exiting debugger.");
                break;
            }
            "c" | "continue" => {
                while cpu.state == State::Running {
                    cpu.tick(1000);
                    while !cpu.terminal_output.is_empty() {
                        let out = cpu.terminal_output.remove(0);
                        println!("  [OUT] {}", out);
                    }
                }
                break;
            }
            "r" | "regs" => {
                println!("Registers: A={:X} B={:X} C={:X} D={:X} PC={} Flags: Z={} S={}",
                    cpu.reg_a, cpu.reg_b, cpu.reg_c, cpu.reg_d, cpu.pc, cpu.flag_z, cpu.flag_s);
                println!("DataStack: {:?}", cpu.data_stack);
                println!("CallStack: {:?}", cpu.call_stack);
            }
            m if m.starts_with('m') => {
                let parts: Vec<&str> = m.split_whitespace().collect();
                let addr: usize = if parts.len() > 1 {
                    parts[1].parse().unwrap_or(0)
                } else {
                    0
                };
                print!("{}", cpu.ram.dump(addr, 16));
            }
            _ => {
                cpu.step();
                while !cpu.terminal_output.is_empty() {
                    let out = cpu.terminal_output.remove(0);
                    println!("  [OUT] {}", out);
                }
            }
        }
    }
}
