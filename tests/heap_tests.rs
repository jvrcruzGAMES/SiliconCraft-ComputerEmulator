use siliconcraft_emulator::{State, VirtualCpu};

fn run(source: &[&str]) -> VirtualCpu {
    let mut cpu = VirtualCpu::default();
    let program: Vec<String> = source.iter().map(|line| line.to_string()).collect();
    cpu.burn_to_rom(&program);
    cpu.execute_cycles(1_000_000);
    assert_eq!(cpu.state, State::Halted, "{:?}", cpu.terminal_output);
    cpu
}

#[test]
fn galloc_returns_zeroed_blocks_above_the_heap_header() {
    let cpu = run(&["MOV [1010], 99", "GALLOC 0, 1000", "GALLOC 10, 1000", "MOV [A], 5", "HLT"]);
    let address = cpu.reg_a as usize;
    assert_eq!(address, 1005);
    assert_eq!(cpu.ram.as_slice()[address], 5);
    assert_eq!(cpu.ram.as_slice()[address + 1], 0);
}

#[test]
fn galloc_collects_unreferenced_blocks() {
    let cpu = run(&[
        "GALLOC 0, 1000",
        "MOV D, 50",
        "again:",
        "GALLOC 1000, 1000",
        "DEC D",
        "JNZ again",
        "HLT",
    ]);
    // Without collection fifty 1002-word blocks would push the heap top past 51000.
    assert!(cpu.ram.as_slice()[1000] < 10_000, "heap top {}", cpu.ram.as_slice()[1000]);
}

#[test]
fn galloc_keeps_blocks_referenced_from_ram_below_the_heap() {
    let cpu = run(&[
        "GALLOC 0, 1000",
        "GALLOC 100, 1000",
        "MOV [10], A",
        "MOV [A], 777",
        "MOV D, 20",
        "again:",
        "GALLOC 1000, 1000",
        "DEC D",
        "JNZ again",
        "MOV B, [10]",
        "MOV C, [B]",
        "HLT",
    ]);
    assert_eq!(cpu.reg_c, 777);
}

#[test]
fn galloc_reports_out_of_memory() {
    let mut cpu = VirtualCpu::default();
    let program = vec!["GALLOC 0, 1000".to_string(), "GALLOC 2147483647, 1000".to_string(), "HLT".to_string()];
    cpu.burn_to_rom(&program);
    cpu.execute_cycles(10);
    assert_ne!(cpu.state, State::Halted);
}

#[test]
fn nutf8_encodes_and_decodes_guest_strings() {
    let cpu = run(&[
        "MOV [10], 233",
        "MOV [11], 0",
        "NUTF8 A, 10, -1, 20, 8",
        "NUTF8 B, 20, 2, 30, 8",
        "NUTF8 C, 10, -1, 0, 0",
        "HLT",
    ]);
    assert_eq!((cpu.reg_a, cpu.reg_b, cpu.reg_c), (2, 1, 2));
    assert_eq!(&cpu.ram.as_slice()[20..22], &[195, 169]);
    assert_eq!(&cpu.ram.as_slice()[30..32], &[233, 0]);
}

#[test]
fn nready_reports_an_idle_mailbox() {
    let cpu = run(&["NREADY A", "HLT"]);
    assert_eq!(cpu.reg_a, 1);
}
