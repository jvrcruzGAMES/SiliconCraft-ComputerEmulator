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
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();
    cpu.load_program_lines(&program).expect("assembly should succeed");
    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.framebuffer.get_pixel(10, 10), 0xFFFF0000);
    assert_eq!(cpu.framebuffer.get_pixel(20, 20), 0xFF00FF00);
    assert_eq!(cpu.framebuffer.get_pixel(25, 22), 0xFF00FF00);
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
fn test_four_sticks_ram_and_nvme_and_rom() {
    let program = vec![
        "MOV [1000000], 12345",  // Address requires > 768K words, impossible without 4 sticks (1M words)
        "MOV A, [1000000]",
        "SSEL 1",               // Select Extension Slot 1 (NVMe drive)
        "SWRITE 0, 42",         // Write 42 to NVMe drive
        "SREAD B, 0",           // Read back into B
        "HLT",
    ];

    let mut cpu = VirtualCpu::default();

    // 1. Verify 4 sticks of RAM: 4096 KB / 4 MB (1,048,576 words)
    assert_eq!(cpu.ram_stick_count(), 4);
    assert_eq!(cpu.ram_capacity_kb(), 4096);
    assert_eq!(cpu.ram.len(), 1_048_576);

    // 2. Verify all instructions enabled
    assert!(cpu.supported_instructions.len() >= 71);
    assert!(cpu.is_instruction_supported("MOV"));
    assert!(cpu.is_instruction_supported("SBLKR"));
    assert!(cpu.is_instruction_supported("FBRECT"));
    assert!(cpu.is_instruction_supported("ATONE"));

    // 3. Verify NVMe in Extension Slot 1 (Drive 1)
    assert!(cpu.has_nvme());
    assert!(cpu.extension_slot_1_is_nvme());
    assert_eq!(cpu.storage.capacity(1), 1024 * 1024 * 1024); // 1024 MB

    // 4. Burn code to ROM and execute
    cpu.burn_to_rom(&program);
    assert!(cpu.is_code_burned_to_rom());
    assert_eq!(cpu.rom().len(), 6);

    cpu.execute_cycles(100);

    assert_eq!(cpu.state, State::Halted);
    assert_eq!(cpu.read_reg(Register::A), 12345);
    assert_eq!(cpu.read_reg(Register::B), 42);
    assert_eq!(cpu.ram.read(1000000), Ok(12345));

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
