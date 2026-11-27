use siliconcraft_emulator::{State, VirtualCpu};

#[test]
fn gpu_draws_on_the_back_buffer_and_presents_on_swap() {
    let mut cpu = VirtualCpu::new(128);
    cpu.burn_to_rom(&[
        "GFXSTAT [10]",
        "GFXSTAT [11], 1",
        "GFXSEL 2",
        "GFXSTAT [12], 2",
        "GFXSTAT [13], 3",
        "GFXSTAT [14], 6",
        "GPUCLR 0",
        "GFXSTAT [15], 7",
        "GPURECT 10, 10, 3, 3, 0x00FF00",
        "GPULINE 20, 20, 22, 22, 0x0000FF",
        "GPUCIRC 30, 30, 3, 0xFFFF00, 0",
        "GPUCIRC 40, 40, 3, 0xFFFF00",
        "GPUTRI 0",
        "GPUVRAM 0, 7, 2",
        "GPUBLIT 50, 50, 0, 2, 1",
        "FBSET 60, 60, 0xFFFFFF",
        "FBLIT 70, 70, 9, 1, 1",
        "MOV [9], 0",
        "GPUSYNC",
        "GFXSTAT [16], 7",
        "FBGET [17], 51, 50",
        "GPUSWAP",
        "FBSET 80, 80, 0xFFFFFF",
        "GPUSYNC",
        "GFXSTAT [18], 8",
        "GFXSTAT [19], 5",
        "HLT",
    ]);
    cpu.ram.as_mut_slice()[..10].copy_from_slice(&[
        100,
        100,
        104,
        100,
        102,
        104,
        0x007755,
        0,
        0x80ff0000u32 as i32,
        0x332211,
    ]);
    cpu.execute_cycles(100);
    assert_eq!(cpu.state, State::Halted, "{:?}", cpu.terminal_output);
    assert_eq!(&cpu.ram.as_slice()[10..17], &[1, 0, 2, 2_097_152, 4, 1, 0]);
    assert_eq!(cpu.ram.read(17).unwrap() as u32, 0xff800000);
    assert_eq!(&cpu.ram.as_slice()[18..20], &[1, 11]);
    let frame = cpu.active_framebuffer();
    assert_eq!(frame.get_pixel(11, 11), 0xff00ff00);
    assert_eq!(frame.get_pixel(21, 21), 0xff0000ff);
    assert_eq!(frame.get_pixel(30, 33), 0xffffff00);
    assert_eq!(frame.get_pixel(30, 30), 0xff000000);
    assert_eq!(frame.get_pixel(40, 40), 0xffffff00);
    assert_eq!(frame.get_pixel(102, 102), 0xff007755);
    assert_eq!(frame.get_pixel(50, 50), 0xff000000);
    assert_eq!(frame.get_pixel(51, 50), 0xff800000);
    assert_eq!(frame.get_pixel(60, 60), 0xffffffff);
    assert_eq!(frame.get_pixel(70, 70), 0xff332211);
    assert_eq!(frame.get_pixel(80, 80), 0xff000000);
}

#[test]
fn graphics_selection_keeps_cpu_and_gpu_frames_separate() {
    let mut cpu = VirtualCpu::new(32);
    cpu.burn_to_rom(&[
        "FBSET 0, 0, 0x11",
        "GFXSEL 0",
        "FBSET 0, 0, 0x22",
        "FBSWAP",
        "GFXSEL 1",
        "HLT",
    ]);
    cpu.execute_cycles(4);
    assert_eq!(cpu.framebuffer.get_pixel(0, 0), 0xff000011);
    assert_eq!(cpu.active_framebuffer().get_pixel(0, 0), 0xff000022);
    cpu.execute_cycles(20);
    assert_eq!(cpu.active_framebuffer().get_pixel(0, 0), 0xff000011);
    assert!(cpu.flag_z);
    assert!(!cpu.flag_s);
}

#[test]
fn gpu_instructions_reject_out_of_bounds_buffers() {
    for instruction in [
        "GPUVRAM 524287, 0, 2",
        "GPUBLIT 0, 0, 524288, 1, 1",
        "GPUTRI 27",
    ] {
        let mut cpu = VirtualCpu::new(32);
        cpu.burn_to_rom(&[instruction, "HLT"]);
        cpu.execute_cycles(10);
        assert!(
            matches!(cpu.state, State::Error(_)),
            "{instruction}: {:?}",
            cpu.state
        );
    }
}

#[test]
fn inaxis_returns_clamped_values_and_neutral_invalid_or_reset_ports() {
    let mut cpu = VirtualCpu::new(32);
    cpu.burn_to_rom(&[
        "INAXIS A, 2, 0",
        "INAXIS B, 2, 4",
        "INAXIS C, -1, 0",
        "INAXIS D, 2, 6",
        "HLT",
    ]);
    cpu.set_gamepad_state(2, 0, [-1200, 0, 0, 0, 1200, 0]);
    cpu.execute_cycles(10);
    assert_eq!(
        (cpu.reg_a, cpu.reg_b, cpu.reg_c, cpu.reg_d),
        (-1000, 1000, 0, 0)
    );
    cpu.reset();
    cpu.execute_cycles(10);
    assert_eq!((cpu.reg_a, cpu.reg_b), (0, 0));
}
