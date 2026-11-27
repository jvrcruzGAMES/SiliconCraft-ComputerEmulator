use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::{EventPump, GamepadSubsystem};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Sample {
    pub buttons: i32,
    pub axes: [i32; 6],
}

pub(super) struct Gamepads {
    events: EventPump,
    devices: [Option<Gamepad>; 4],
    subsystem: GamepadSubsystem,
}

impl Gamepads {
    pub(super) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let sdl = sdl3::init()?;
        let subsystem = sdl.gamepad()?;
        Ok(Self {
            events: sdl.event_pump()?,
            devices: [None, None, None, None],
            subsystem,
        })
    }

    pub(super) fn poll(&mut self) -> [Sample; 4] {
        // Winit owns the window. SDL processes controller events only.
        for _ in self.events.poll_iter() {}
        self.subsystem.update();
        for slot in &mut self.devices {
            if slot.as_ref().is_some_and(|pad| !pad.connected()) {
                *slot = None;
            }
        }
        if let Ok(ids) = self.subsystem.gamepads() {
            for id in ids {
                if !self
                    .devices
                    .iter()
                    .flatten()
                    .any(|pad| pad.id().ok() == Some(id))
                {
                    if let Some(slot) = self.devices.iter_mut().find(|slot| slot.is_none()) {
                        *slot = self.subsystem.open(id).ok();
                    }
                }
            }
        }
        std::array::from_fn(|port| {
            self.devices[port]
                .as_ref()
                .map_or_else(Sample::default, controller_sample)
        })
    }
}

fn controller_sample(pad: &Gamepad) -> Sample {
    let sample = |axis, trigger| {
        let raw = f64::from(pad.axis(axis));
        let normalized = raw / if raw < 0.0 { 32768.0 } else { 32767.0 };
        if trigger {
            (normalized.max(0.0) * 1000.0 + 0.5).floor() as i32
        } else if normalized.abs() <= 0.15 {
            0
        } else {
            (normalized * 1000.0 + 0.5).floor() as i32
        }
    };
    let axes = [
        sample(Axis::LeftX, false),
        sample(Axis::LeftY, false),
        sample(Axis::RightX, false),
        sample(Axis::RightY, false),
        sample(Axis::TriggerLeft, true),
        sample(Axis::TriggerRight, true),
    ];
    let [x, y, _, _, left_trigger, right_trigger] = axes;
    let pressed = [
        pad.button(Button::DPadRight) || x > 500,
        pad.button(Button::DPadLeft) || x < -500,
        pad.button(Button::DPadDown) || y > 500,
        pad.button(Button::DPadUp) || y < -500,
        pad.button(Button::Start),
        pad.button(Button::Back),
        pad.button(Button::South),
        pad.button(Button::East),
        pad.button(Button::West),
        pad.button(Button::North),
        pad.button(Button::LeftShoulder) || left_trigger > 500,
        pad.button(Button::RightShoulder) || right_trigger > 500,
        left_trigger > 500,
        right_trigger > 500,
        pad.button(Button::LeftStick),
        pad.button(Button::RightStick),
    ];
    let buttons = pressed
        .into_iter()
        .enumerate()
        .fold(0, |mask, (bit, down)| mask | (i32::from(down) << bit));
    Sample { buttons, axes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualCpu;
    use sdl3::joystick::{JoystickType, VirtualJoystickDescription};

    #[test]
    fn virtual_controller_sets_and_releases_guest_buttons() {
        let mut pads = Gamepads::new().unwrap();
        let sdl = sdl3::init().unwrap();
        let joysticks = sdl.joystick().unwrap();
        let description = VirtualJoystickDescription::new()
            .name("SiliconCraft test")
            .joystick_type(JoystickType::Gamepad)
            .with_axis(Axis::LeftX)
            .with_button(Button::South);
        let virtual_pad = joysticks.attach_virtual_joystick(description).unwrap();
        let joystick = joysticks.open(virtual_pad.id()).unwrap();
        joystick
            .set_virtual_axis(Axis::LeftX as u32, 20_000)
            .unwrap();
        joystick
            .set_virtual_button(Button::South as u32, true)
            .unwrap();

        let mut cpu = VirtualCpu::default();
        cpu.burn_to_rom(&["INPAD A", "INAXIS B, 0, 0", "HLT"]);
        let sample = pads.poll()[0];
        cpu.set_gamepad_state(0, sample.buttons, sample.axes);
        cpu.execute_cycles(10);
        assert_eq!(cpu.reg_a, 1 | (1 << 6));

        assert_eq!(cpu.reg_b, 610);

        drop(virtual_pad);
        assert_eq!(pads.poll(), [Sample::default(); 4]);
    }
}
