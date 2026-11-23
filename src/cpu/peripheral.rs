use std::collections::BTreeMap;

/// A device supplied by the emulator host at a SiliconCraft peripheral address.
pub trait Peripheral {
    fn automatic(&self) -> bool {
        false
    }

    fn call(&mut self, _method: i32, _arg0: i32, _arg1: i32) -> Option<i32> {
        None
    }

    fn call_string(&mut self, _method: i32, _text: &str, _arg1: i32) -> Option<i32> {
        None
    }
}

pub(super) enum Operation {
    Info,
    Wrap,
    Call(i32, i32, i32),
    CallString(i32, String, i32),
    Scan,
}

#[derive(Default)]
pub struct PeripheralBus {
    devices: BTreeMap<i32, (Box<dyn Peripheral>, bool)>,
    pending: Option<(i32, Operation)>,
    result: Option<(i32, i32)>,
}

impl PeripheralBus {
    pub fn attach(&mut self, address: i32, device: impl Peripheral + 'static) {
        assert!(address >= 0, "Peripheral addresses must be nonnegative");
        self.devices.insert(address, (Box::new(device), false));
    }

    pub fn detach(&mut self, address: i32) {
        self.devices.remove(&address);
    }

    pub(super) fn reset(&mut self) {
        self.pending = None;
        self.result = None;
        for (_, wrapped) in self.devices.values_mut() {
            *wrapped = false;
        }
    }

    pub(super) fn submit(&mut self, address: i32, operation: Operation) {
        self.pending = Some((address, operation));
    }

    pub(super) fn status(&self) -> i32 {
        self.result
            .map_or(i32::from(self.pending.is_some()), |(status, _)| status)
    }

    pub(super) fn take_result(&mut self) -> i32 {
        self.result.take().map_or(0, |(_, value)| value)
    }

    pub(super) fn service(&mut self) {
        if let Some((address, operation)) = self.pending.take() {
            self.result = Some(self.execute(address, operation));
        }
    }

    fn execute(&mut self, address: i32, operation: Operation) -> (i32, i32) {
        if matches!(operation, Operation::Scan) {
            let next = self
                .devices
                .range((
                    std::ops::Bound::Excluded(address),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map_or(-1, |(&port, _)| port);
            return (2, next);
        }
        if address < 0 {
            return (-4, 0);
        }
        let Some((device, wrapped)) = self.devices.get_mut(&address) else {
            return (-1, 0);
        };
        let automatic = device.automatic();
        let callable = automatic || *wrapped;
        let value = match operation {
            Operation::Info => Some(1 | (i32::from(callable) << 1) | (i32::from(automatic) << 2)),
            Operation::Wrap => {
                *wrapped = true;
                Some(1)
            }
            Operation::Call(_, _, _) | Operation::CallString(_, _, _) if !callable => {
                return (-2, 0)
            }
            Operation::Call(method, arg0, arg1) => device.call(method, arg0, arg1),
            Operation::CallString(method, text, arg1) => device.call_string(method, &text, arg1),
            Operation::Scan => unreachable!(),
        };
        value.map_or((-3, 0), |value| (2, value))
    }
}
