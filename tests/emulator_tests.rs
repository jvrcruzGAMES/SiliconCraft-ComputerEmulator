use siliconcraft_emulator::{VirtualCpu, Register, State, ProjectResolver};
use std::collections::HashMap;

#[test]
fn test_basic_arithmetic() {
    let program = vec![
        "MOV A, 10",
        "MOV B, 25",
        "ADD A, B",
        "SUB B, 5",
        "MUL A, 2",
        "DIV A, 7",
        "MOD A, 4",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::B), 20);
    // A: 10 + 25 = 35 -> * 2 = 70 -> / 7 = 10 -> % 4 = 2
    assert_eq!(cpu.read_reg(Register::A), 2);
}

#[test]
fn test_conditional_jumps_and_loops() {
    // Computes sum of numbers from 1 to 5: 1+2+3+4+5 = 15
    let program = vec![
        "MOV A, 0",
        "MOV B, 1",
        "loop:",
        "ADD A, B",
        "INC B",
        "CMP B, 5",
        "JLE loop",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(200);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 15);
    assert_eq!(cpu.read_reg(Register::B), 6);
}

#[test]
fn test_call_and_ret() {
    let program = vec![
        "MOV A, 5",
        "CALL double_a",
        "ADD A, 3",
        "HLT",
        "double_a:",
        "MUL A, 2",
        "RET",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 13);
}

#[test]
fn test_dynamic_register_jumps() {
    let program = vec![
        "MOV B, target",
        "JMP B",
        "MOV A, 999", // should be skipped
        "target:",
        "MOV A, 42",
        "MOV C, subroutine",
        "CALL C",
        "HLT",
        "subroutine:",
        "ADD A, 8",
        "RET",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 50);
}

#[test]
fn test_push_pop_and_pusha_popa() {
    let program = vec![
        "MOV A, 11",
        "MOV B, 22",
        "MOV C, 33",
        "MOV D, 44",
        "PUSHA",
        "MOV A, 0",
        "MOV B, 0",
        "MOV C, 0",
        "MOV D, 0",
        "POPA",
        "PUSH A",
        "POP D",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 11);
    assert_eq!(cpu.read_reg(Register::B), 22);
    assert_eq!(cpu.read_reg(Register::C), 33);
    assert_eq!(cpu.read_reg(Register::D), 11);
}

#[test]
fn test_static_data_directives() {
    let program = vec![
        "ORG 10",
        "DB 0x12, 0x34, 'A'",
        "DW 0x5678, 0x1234",
        "DL 0x9ABCDEF0",
        "ORG 0",
        "MOV A, [10]",
        "MOV B, [13]",
        "MOV C, [15]",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.ram.read(10), Ok(0x12));
    assert_eq!(cpu.ram.read(11), Ok(0x34));
    assert_eq!(cpu.ram.read(12), Ok(b'A' as i32));
    assert_eq!(cpu.ram.read(13), Ok(0x5678));
    assert_eq!(cpu.ram.read(14), Ok(0x1234));
    assert_eq!(cpu.ram.read(15), Ok(0x9ABCDEF0u32 as i32));

    assert_eq!(cpu.read_reg(Register::A), 0x12);
    assert_eq!(cpu.read_reg(Register::B), 0x5678);
    assert_eq!(cpu.read_reg(Register::C), 0x9ABCDEF0u32 as i32);
}

#[test]
fn test_software_interrupt() {
    let program = vec![
        "IVEC 5, isr",
        "STI",
        "MOV A, 100",
        "INT 5",
        "ADD A, 1",
        "HLT",
        "isr:",
        "ADD A, 50",
        "IRET",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    // INT 5 passes vector 5 in reg_a; isr adds 50 -> 55; IRET keeps reg_a; ADD A, 1 -> 56
    assert_eq!(cpu.read_reg(Register::A), 56);
}

#[test]
fn test_storage_block_transfer() {
    let program = vec![
        "MOV [0], 100",
        "MOV [1], 200",
        "MOV [2], 250",
        "SBLKW 0, 0, 0, 3",  // drive 0, storage_addr 0, ram_src 0, count 3
        "MOV [0], 0",
        "MOV [1], 0",
        "MOV [2], 0",
        "SBLKR 0, 0, 0, 3",  // ram_dest 0, drive 0, storage_addr 0, count 3
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.ram.read(0), Ok(100));
    assert_eq!(cpu.ram.read(1), Ok(200));
    assert_eq!(cpu.ram.read(2), Ok(250));
}

#[test]
fn test_graphics_and_framebuffer() {
    let program = vec![
        "FBMODE 1",
        "FBCLR 0xFF000000",
        "FBSET 10, 10, 0xFFFF0000",
        "FBRECT 20, 20, 10, 5, 0xFF00FF00",
        "FBWIDTH A",
        "FBHEIGHT B",
        "FBSET 959, 539, 0xFFABCDEF",
        "MOV [100], 0x00335577",
        "FBLIT 800, 500, 100, 1, 1",
        "FBLINE -2147483648, 400, 2147483647, 400, 0x00001133",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.framebuffer.get_pixel(10, 10), 0xFFFF0000);
    assert_eq!(cpu.framebuffer.get_pixel(20, 20), 0xFF00FF00);
    assert_eq!(cpu.framebuffer.get_pixel(25, 22), 0xFF00FF00);
    assert_eq!(cpu.read_reg(Register::A), 960);
    assert_eq!(cpu.read_reg(Register::B), 540);
    assert_eq!(cpu.framebuffer.get_pixel(959, 539), 0xFFABCDEF);
    assert_eq!(cpu.framebuffer.get_pixel(960, 539), 0);
    assert_eq!(cpu.framebuffer.get_pixel(800, 500), 0xFF335577);
    assert_eq!(cpu.framebuffer.get_pixel(959, 400), 0xFF001133);
}

#[test]
fn test_audio_playback() {
    let program = vec![
        "ATONE 440, 100",
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert!(cpu.audio.recorded_samples().len() > 0);
}

#[test]
fn test_maximum_ram_and_storage_and_rom() {
    let program = vec![
        "MOV [4194303], 12345",  // Address requires > 768K words, impossible without 4 sticks (1M words)
        "MOV A, [4194303]",
        "SSEL 1",               // Select Extension Slot 1 (NVMe drive)
        "SWRITE 0, 42",         // Write 42 to NVMe drive
        "SREAD B, 0",           // Read back into B
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();

    // 1. Verify 4 sticks of RAM: 4096 KB / 4 MB (1,048,576 words)
    assert_eq!(cpu.ram_stick_count(), 8);
    assert_eq!(cpu.ram_capacity_kb(), 16_384);
    assert_eq!(cpu.ram.len(), 4_194_304);

    // 2. Verify all instructions enabled
    assert!(cpu.supported_instructions.len() >= 71);
    assert!(cpu.is_instruction_supported("MOV"));
    assert!(cpu.is_instruction_supported("SBLKR"));
    assert!(cpu.is_instruction_supported("FBRECT"));
    assert!(cpu.is_instruction_supported("ATONE"));

    // 3. Verify NVMe in Extension Slot 1 (Drive 1)
    assert!(cpu.has_nvme());
    assert!(cpu.extension_slot_1_is_nvme());
    assert_eq!(cpu.storage.capacity(1), 2 * 1024 * 1024 * 1024);
    assert_eq!(cpu.storage.capacity(2), cpu.storage.capacity(0));
    assert_eq!(cpu.storage.capacity(3), 0);
    assert_eq!(cpu.storage.drive_count(), 3);

    // 4. Burn code to ROM and execute
    cpu.burn_to_rom(&program);
    assert!(cpu.is_code_burned_to_rom());
    assert_eq!(cpu.rom().len(), 6);

    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 12345);
    assert_eq!(cpu.read_reg(Register::B), 42);
    assert_eq!(cpu.ram.read(4194303), Ok(12345));

    // 5. Verify ROM non-volatility on CPU reset
    cpu.reset();
    assert!(cpu.is_code_burned_to_rom());
    assert_eq!(cpu.pc, 0);
    assert_eq!(cpu.read_reg(Register::A), 0);
}

#[test]
fn test_project_n_imports_assembly() {
    let mut files = HashMap::new();
    files.insert(
        "main.n".to_string(),
        vec![
            r#"import "math_sub.asm";"#.to_string(),
            "fn main() {".to_string(),
            "    A = 20".to_string(),
            "    call(add_ten)".to_string(),
            "}".to_string(),
            "main()".to_string(),
            "asm {".to_string(),
            "    HLT".to_string(),
            "}".to_string(),
        ],
    );
    files.insert(
        "math_sub.asm".to_string(),
        vec![
            "add_ten:".to_string(),
            "ADD A, 10".to_string(),
            "RET".to_string(),
        ],
    );

    let compiled = ProjectResolver::compile_project(&files, None).expect("Project compilation should succeed");
    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&compiled).expect("Assembly should succeed");
    cpu.execute_cycles(200);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 30);
}

#[test]
fn test_pure_assembly_project_imports() {
    let mut files = HashMap::new();
    files.insert(
        "main.asm".to_string(),
        vec![
            r#"include "helper.asm""#.to_string(),
            "MOV A, 5".to_string(),
            "CALL triple_a".to_string(),
            "HLT".to_string(),
        ],
    );
    files.insert(
        "helper.asm".to_string(),
        vec![
            "triple_a:".to_string(),
            "MUL A, 3".to_string(),
            "RET".to_string(),
        ],
    );

    let compiled = ProjectResolver::compile_project(&files, None).expect("Project compilation should succeed");
    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&compiled).expect("Assembly should succeed");
    cpu.execute_cycles(200);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 15);
}

#[test]
fn test_assembly_imports_n_project() {
    let mut files = HashMap::new();
    files.insert(
        "main.asm".to_string(),
        vec![
            r#"include "logic.n""#.to_string(),
            "CALL compute".to_string(),
            "ADD A, 5".to_string(),
            "HLT".to_string(),
        ],
    );
    files.insert(
        "logic.n".to_string(),
        vec![
            "fn compute() {".to_string(),
            "    A = 7".to_string(),
            "    B = 8".to_string(),
            "    A = A + B".to_string(),
            "}".to_string(),
        ],
    );

    let compiled = ProjectResolver::compile_project(&files, None).expect("Project compilation should succeed");
    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&compiled).expect("Assembly should succeed");
    cpu.execute_cycles(200);

    assert_eq!(cpu.state, State::Halted);
    // 7 + 8 = 15 -> + 5 = 20
    assert_eq!(cpu.read_reg(Register::A), 20);
}

#[test]
fn test_emulator_window_creation() {
    let program = vec![
        r#"OUT "Hello Vulkan!""#.to_string(),
        "FBMODE 1".to_string(),
        "FBSET 50, 50, 0xFFFF0000".to_string(),
        "HLT".to_string(),
    ];
    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("Program should load");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.framebuffer.mode(), 1);
    assert_eq!(cpu.framebuffer.get_pixel(50, 50), 0xFFFF0000);

    // Verify EmulatorWindow initializes correctly with the CPU
    let _win = siliconcraft_emulator::EmulatorWindow::new(cpu);
}

#[test]
fn test_reset_interrupt_and_ram_preservation() {
    let program = vec![
        "IVEC RESET, on_reset".to_string(),
        "MOV [100], 42".to_string(),
        "HLT".to_string(),
        "on_reset:".to_string(),
        "MOV A, [100]".to_string(),
        "MOV [101], A".to_string(),
        "IRET".to_string(),
    ];
    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("Program should load");
    cpu.execute_cycles(20);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.ram.read(100), Ok(42));
    assert_eq!(cpu.ram.read(101), Ok(0));

    // Reset CPU
    cpu.reset();

    // RAM must persist across reset
    assert_eq!(cpu.ram.read(100), Ok(42));

    // Execute cycles: reset interrupt handler must run and copy [100] -> [101]
    cpu.execute_cycles(20);
    assert_eq!(cpu.ram.read(101), Ok(42));
}

#[test]
fn storage_instructions_handle_the_last_byte_and_signed_capacity() {
    let mut cpu = VirtualCpu::default();
    cpu.burn_to_rom(&[
        "SSIZE A, 2",
        "MOV [10], 255",
        "SBLKW 2, 2147483647, 10, 2",
        "SREAD B, 2, 2147483647",
        "SREAD C, 2, 2147483646",
        "SREAD D, 3, 0",
        "HLT",
    ]);
    cpu.execute_cycles(50);
    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.reg_a, i32::MIN);
    assert_eq!(cpu.reg_b, 255);
    assert_eq!(cpu.reg_c, 0);
    assert_eq!(cpu.reg_d, 0);
    assert!(cpu.flag_z && cpu.flag_s);

    cpu.reset();
    assert_eq!(cpu.storage.read_byte(2, i32::MAX as usize), Ok(255));
}

#[test]
fn storage_can_boot_code_from_bytes_without_overwriting_rom() {
    let mut cpu = VirtualCpu::default();
    for (index, value) in b"MOV A, 42\nSDRV B\nHLT".iter().enumerate() {
        cpu.storage.write_byte(2, index, i32::from(*value)).unwrap();
    }
    cpu.burn_to_rom(&["SEXEC 2"]);
    cpu.execute_cycles(50);
    assert_eq!(cpu.state, State::Halted);
    assert_eq!((cpu.reg_a, cpu.reg_b), (42, 2));
    assert_eq!(cpu.rom(), ["SEXEC 2"]);
}

#[test]
fn inpad_reads_connected_ports_and_keyboard_queue() {
    let mut cpu = VirtualCpu::default();
    cpu.burn_to_rom(&[
        "INPAD A, -2", "INPAD B, -1", "INPAD C, -1",
        "INPAD D, 1", "INPAD [0], 4", "HLT",
    ]);
    cpu.set_pad_state(1, 2049);
    cpu.queue_keyboard_interrupt(90);
    cpu.execute_cycles(50);
    assert_eq!((cpu.reg_a, cpu.reg_b, cpu.reg_c, cpu.reg_d), (90, 90, -1, 2049));
    assert_eq!(cpu.ram.read(0), Ok(0));
}

#[test]
fn peripheral_instructions_discover_wrap_and_call_devices() {
    use siliconcraft_emulator::cpu::peripheral::Peripheral;

    struct Device;
    impl Peripheral for Device {
        fn call(&mut self, method: i32, arg0: i32, arg1: i32) -> Option<i32> {
            (method == 1).then_some(arg0 + arg1)
        }

        fn call_string(&mut self, method: i32, text: &str, arg: i32) -> Option<i32> {
            (method == 2 && text == "abc").then_some(arg + 3)
        }
    }

    let mut cpu = VirtualCpu::default();
    cpu.peripheral_bus.attach(7, Device);
    cpu.burn_to_rom(&[
        "PSCAN A, -1", "PRESULT [0]",
        "PINFO A, 7", "PSTATUS [1]", "PRESULT [2]",
        "PCALL A, 7, 1, 10, 20", "PSTATUS [3]", "PRESULT A",
        "PWRAP A, 7", "PRESULT B",
        "PCALL A, 7, 1, 10, 20", "PRESULT [4]",
        "MOV [20], 97", "MOV [21], 98", "MOV [22], 99",
        "PCALLS A, 7, 2, 20, 40", "PRESULT [5]",
        "PSTATUS [6]", "HLT",
    ]);
    cpu.execute_cycles(100);
    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.reg_b, 1);
    assert_eq!(&cpu.ram.as_slice()[..7], &[7, 2, 1, -2, 30, 43, 0]);
}

#[test]
fn network_is_disabled_without_an_allowed_endpoint() {
    let mut cpu = VirtualCpu::default();
    cpu.burn_to_rom(&["NOPEN A, 1, 10, 80", "NSTATUS B", "NLEN C", "NRESULT D, 0, 0", "HLT"]);
    cpu.execute_cycles(100);
    assert_eq!(cpu.state, State::Halted);
    assert_eq!((cpu.reg_a, cpu.reg_b, cpu.reg_c, cpu.reg_d), (1, -1, 0, 0));
}

#[test]
fn sparse_image_retains_bytes_when_reopened() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nvme.img");
    {
        let mut cpu = VirtualCpu::default();
        cpu.storage.attach_image(1, &path).unwrap();
        cpu.burn_to_rom(&["SWRITE 1, 2147483647, 197", "HLT"]);
        cpu.execute_cycles(10);
        assert_eq!(cpu.state, State::Halted);
        assert!(!cpu.flag_s);
    }
    let mut cpu = VirtualCpu::default();
    cpu.storage.attach_image(1, &path).unwrap();
    assert_eq!(cpu.storage.read_byte(1, 0), Ok(0));
    assert_eq!(cpu.storage.read_byte(1, i32::MAX as usize), Ok(197));
}

#[test]
fn outc_decodes_utf8_and_preserves_blank_lines() {
    let mut cpu = VirtualCpu::default();
    cpu.burn_to_rom(&["OUTC 195", "OUTC 161", "OUTC 10", "OUTC 10", "OUTC \"Olá\"", "HLT"]);
    cpu.execute_cycles(20);
    assert_eq!(&cpu.terminal_output[..3], &["á", "", "Olá"]);
}

#[test]
fn peripheral_text_instructions_use_utf16_and_consume_the_mailbox() {
    use siliconcraft_emulator::cpu::peripheral::{Peripheral, PeripheralResponse};

    struct Device;
    impl Peripheral for Device {
        fn automatic(&self) -> bool {
            true
        }

        fn invoke(&mut self, method: i32, _: i32, _: i32) -> Option<PeripheralResponse> {
            let text = if method == 0 { "😀Z".to_owned() } else { "x".repeat(8193) };
            Some(PeripheralResponse::text(text))
        }

        fn invoke_string(&mut self, _: i32, text: &str, _: i32) -> Option<PeripheralResponse> {
            Some(PeripheralResponse::text(text.to_owned()))
        }
    }

    let mut cpu = VirtualCpu::new(400);
    cpu.peripheral_bus.attach(0, Device);
    cpu.burn_to_rom(&[
        "PCALL A, 0, 0, 0, 0",
        "PLEN [0]",
        "PLEN [1]",
        "PRESULTS [2], 10, 3",
        "PSTATUS [3]",
        "PLEN [4]",
        "PCALL A, 0, 0, 0, 0",
        "PRESULT [5]",
        "PLEN [6]",
        "PCALL A, 0, 1, 0, 0",
        "PSTATUS [7]",
        "PRESULTS [8], 400, 0",
        "PSTATUS [9]",
        "PCALLS A, 0, 0, 100, 0",
        "PLEN [13]",
        "PRESULTS [14], 20, 80",
        "HLT",
    ]);
    cpu.ram.as_mut_slice()[10..14].fill(-1);
    cpu.ram.as_mut_slice()[100..170].fill(65);
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(&cpu.ram.as_slice()[..10], &[3, 3, 2, 0, 0, 3, 0, -4, 0, 0]);
    assert_eq!(&cpu.ram.as_slice()[10..13], &[0xd83d, 0xde00, 0]);
    assert_eq!(&cpu.ram.as_slice()[13..15], &[70, 70]);
    assert_eq!(cpu.ram.read(20), Ok(65));
    assert_eq!(cpu.ram.read(90), Ok(0));
}

#[test]
fn cpu_limits_stack_depth_and_keeps_existing_entries_on_overflow() {
    let mut cpu = VirtualCpu::new(32);
    cpu.burn_to_rom(&["PUSHA", "HLT"]);
    cpu.data_stack.resize(65_534, 17);
    cpu.execute_cycles(10);
    assert!(matches!(&cpu.state, State::Error(message) if message.contains("Data stack overflow")));
    assert_eq!(cpu.data_stack.len(), 65_534);

    cpu.burn_to_rom(&["CALL 0"]);
    cpu.call_stack.resize(65_536, 0);
    cpu.execute_cycles(10);
    assert!(matches!(&cpu.state, State::Error(message) if message.contains("Call stack overflow")));
    assert_eq!(cpu.call_stack.len(), 65_536);

    cpu.burn_to_rom(&["IVEC 42, 1", "INT 42"]);
    for _ in 0..1030 {
        cpu.step();
    }
    assert_eq!(cpu.state, State::Error("Interrupt stack overflow".into()));
    assert_eq!(cpu.interrupt_stack.len(), 1024);
}

#[test]
fn audio_limits_pending_samples_and_keeps_newest_data() {
    use siliconcraft_emulator::hardware::audio::{AudioSubsystem, MAX_AUDIO_QUEUE};

    let mut audio = AudioSubsystem::new();
    audio.set_recording(false);
    audio.generate_tone(0, i32::MAX);
    audio.push_sample(7);
    assert_eq!(audio.stat(), MAX_AUDIO_QUEUE);

    let samples = audio.poll_samples().unwrap();
    assert_eq!(samples.len(), MAX_AUDIO_QUEUE);
    assert!(samples[..MAX_AUDIO_QUEUE - 1].iter().all(|&sample| sample == 128));
    assert_eq!(samples.last(), Some(&7));
    assert!(audio.recorded_samples().is_empty());
}

#[test]
fn terminal_flushes_long_continuations_and_limits_scrollback() {
    let mut cpu = VirtualCpu::new(32);
    let chunk = "é".repeat(2048);
    cpu.burn_to_rom(&[format!("OUT \"{chunk}\", 0"), format!("OUT \"{chunk}\", 0")]);
    cpu.execute_cycles(2);
    assert_eq!(cpu.terminal_output, [chunk.repeat(2)]);
    assert!(cpu.line_buffer.is_empty());

    let lines = (0..70).map(|index| format!("OUT {index}")).collect::<Vec<_>>();
    cpu.burn_to_rom(&lines);
    cpu.execute_cycles(70);
    assert_eq!(cpu.terminal_output.len(), 60);
    assert_eq!(cpu.terminal_output.first().map(String::as_str), Some("10"));
    assert_eq!(cpu.terminal_output.last().map(String::as_str), Some("69"));
}
