use siliconcraft_emulator::{VirtualCpu, Register, State};

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
