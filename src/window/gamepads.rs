use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::{EventPump, GamepadSubsystem};

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

    pub(super) fn poll(&mut self) -> [i32; 4] {
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
        std::array::from_fn(|port| self.devices[port].as_ref().map_or(0, controller_mask))
    }
}

fn controller_mask(pad: &Gamepad) -> i32 {
    let x = pad.axis(Axis::LeftX);
    let y = pad.axis(Axis::LeftY);
    let pressed = [
        pad.button(Button::DPadRight) || x > 16_383,
        pad.button(Button::DPadLeft) || x < -16_384,
        pad.button(Button::DPadDown) || y > 16_383,
        pad.button(Button::DPadUp) || y < -16_384,
        pad.button(Button::Start),
        pad.button(Button::Back),
        pad.button(Button::South),
        pad.button(Button::East),
        pad.button(Button::West),
        pad.button(Button::North),
        pad.button(Button::LeftShoulder) || pad.axis(Axis::TriggerLeft) > 16_383,
        pad.button(Button::RightShoulder) || pad.axis(Axis::TriggerRight) > 16_383,
    ];
    pressed
        .into_iter()
        .enumerate()
        .fold(0, |mask, (bit, down)| mask | (i32::from(down) << bit))
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
        cpu.burn_to_rom(&["INPAD A", "HLT"]);
        cpu.pad_states = pads.poll();
        cpu.execute_cycles(10);
        assert_eq!(cpu.reg_a, 1 | (1 << 6));

        drop(virtual_pad);
        assert_eq!(pads.poll(), [0; 4]);
    }
}
