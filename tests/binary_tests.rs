use siliconcraft_emulator::{State, VirtualCpu};

fn image() -> Vec<u8> {
    let instruction = |opcode: u64, format: u64, flags: u64, payload: u64| {
        (opcode << 56) | (format << 52) | (flags << 48) | payload
    };
    let words = [
        instruction(2, 1, 4, (24 << 24) | 0xff_ffee), // MOV [24], -18
        instruction(2, 3, 2, 0),                      // MOV [A+1], 0xFF112233
        instruction(35, 2, 0, 0),                     // OUT "value: ", cont
        instruction(36, 0, 1, (5 << 45) | 1),         // OUTH [A+1], cont
        instruction(35, 2, 1, 1),                     // OUT " ready"
        instruction(26, 0, 1, (3 << 45) | 30),        // CALL [30]
        instruction(70, 0, 0, 0),                     // HLT
        instruction(35, 0, 1, (3 << 45) | (1 << 32) | 24), // OUT [24]
        instruction(27, 0, 0, 0),                     // RET
    ];
    let mut bytes = b"SCBC\x01\x00\x00\x00".to_vec();
    bytes.extend((words.len() as u32).to_le_bytes());
    for word in words {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(2u32.to_le_bytes());
    for text in ["value: ", " ready"] {
        bytes.extend((text.len() as u32).to_le_bytes());
        bytes.extend(text.as_bytes());
    }
    bytes.extend(2u32.to_le_bytes());
    bytes.extend([5, 0, 0, 0]);
    bytes.extend(1i32.to_le_bytes());
    bytes.extend([2, 0, 0, 0]);
    bytes.extend(0xff112233u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(30u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.push(7);
    bytes
}

fn package(image: &[u8]) -> Vec<u8> {
    let mut bytes = b"SCPK\x03\x00\x00\x00\x02\x00\x00\x00".to_vec();
    for (id, offset, length) in [(1u16, 0u32, 36u32), (3, 36, image.len() as u32)] {
        bytes.extend(id.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(offset.to_le_bytes());
        bytes.extend(length.to_le_bytes());
    }
    bytes.extend(image);
    bytes
}

#[test]
fn binary_loads_all_operand_formats_and_restores_static_data_on_reset() {
    for bytes in [image(), package(&image())] {
        let mut cpu = VirtualCpu::new(128);
        cpu.load_binary(&bytes).unwrap();
        cpu.execute_cycles(30);
        assert_eq!(cpu.state, State::Halted, "{:?}", cpu.terminal_output);
        assert_eq!(
            &cpu.terminal_output[..2],
            &["value: 0xFF112233 ready", "-18"]
        );
        assert_eq!(cpu.ram.read(1), Ok(0xff112233u32 as i32));
        cpu.ram.write(30, 0).unwrap();
        cpu.ram.write(40, 99).unwrap();
        cpu.reset();
        assert_eq!(cpu.ram.read(30), Ok(7));
        assert_eq!(cpu.ram.read(40), Ok(99));
    }
}

#[test]
fn sexec_loads_binary_from_drive_without_replacing_rom_or_resetting_registers() {
    for bytes in [image(), package(&image())] {
        let mut cpu = VirtualCpu::new(128);
        cpu.burn_to_rom(&["MOV A, 10", "SEXEC 1", "HLT"]);
        for (offset, byte) in bytes.into_iter().enumerate() {
            cpu.storage.write_byte(1, offset, i32::from(byte)).unwrap();
        }
        cpu.execute_cycles(30);
        assert_eq!(cpu.state, State::Halted, "{:?}", cpu.terminal_output);
        assert_eq!(cpu.rom(), ["MOV A, 10", "SEXEC 1", "HLT"]);
        assert_eq!(cpu.reg_a, 10);
        assert_eq!(cpu.storage.selected_drive(), 1);
        assert_eq!(cpu.ram.read(11), Ok(0xff112233u32 as i32));
        assert_eq!(
            &cpu.terminal_output[..2],
            &["value: 0xFF112233 ready", "-18"]
        );
    }
}

#[test]
fn invalid_binary_does_not_replace_the_current_program() {
    let mut cpu = VirtualCpu::new(128);
    cpu.burn_to_rom(&["MOV A, 42", "HLT"]);
    let valid = image();
    for invalid in [
        valid[..valid.len() - 1].to_vec(),
        b"SCBC\x02\0\0\0".to_vec(),
    ] {
        assert!(cpu.load_binary(&invalid).is_err());
    }
    let mut overlap = package(&valid);
    overlap[28..32].copy_from_slice(&12u32.to_le_bytes());
    assert!(cpu.load_binary(&overlap).is_err());
    cpu.execute_cycles(10);
    assert_eq!(cpu.reg_a, 42);
    assert_eq!(cpu.state, State::Halted);
}
