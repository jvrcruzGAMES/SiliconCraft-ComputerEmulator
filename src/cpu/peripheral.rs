use std::collections::BTreeMap;

const MAX_TEXT_RESULT: usize = 8192;

#[derive(Default)]
pub struct PeripheralResponse {
    pub value: i32,
    pub text: String,
}

impl PeripheralResponse {
    pub fn number(value: i32) -> Self {
        Self {
            value,
            text: String::new(),
        }
    }

    pub fn text(text: String) -> Self {
        Self {
            value: text.encode_utf16().count() as i32,
            text,
        }
    }
}

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

    /// Override to return text or a numeric value and text from a PCALL.
    fn invoke(&mut self, method: i32, arg0: i32, arg1: i32) -> Option<PeripheralResponse> {
        self.call(method, arg0, arg1)
            .map(PeripheralResponse::number)
    }

    /// Override to return text or a numeric value and text from a PCALLS.
    fn invoke_string(&mut self, method: i32, text: &str, arg1: i32) -> Option<PeripheralResponse> {
        self.call_string(method, text, arg1)
            .map(PeripheralResponse::number)
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
    result: Option<(i32, PeripheralResponse)>,
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
            .as_ref()
            .map_or(i32::from(self.pending.is_some()), |(status, _)| *status)
    }

    pub(super) fn take_result(&mut self) -> i32 {
        self.result.take().map_or(0, |(_, response)| response.value)
    }

    pub(super) fn text_length(&self) -> i32 {
        self.result.as_ref().map_or(0, |(_, response)| {
            response.text.encode_utf16().count() as i32
        })
    }

    pub(super) fn take_text(&mut self) -> String {
        self.result
            .take()
            .map_or_else(String::new, |(_, response)| response.text)
    }

    pub(super) fn service(&mut self) {
        if let Some((address, operation)) = self.pending.take() {
            self.result = Some(self.execute(address, operation));
        }
    }

    fn execute(&mut self, address: i32, operation: Operation) -> (i32, PeripheralResponse) {
        if matches!(operation, Operation::Scan) {
            let next = self
                .devices
                .range((
                    std::ops::Bound::Excluded(address),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map_or(-1, |(&port, _)| port);
            return (2, PeripheralResponse::number(next));
        }
        if address < 0 {
            return (-4, PeripheralResponse::default());
        }
        let Some((device, wrapped)) = self.devices.get_mut(&address) else {
            return (-1, PeripheralResponse::default());
        };
        let automatic = device.automatic();
        let callable = automatic || *wrapped;
        let response = match operation {
            Operation::Info => Some(PeripheralResponse::number(
                1 | (i32::from(callable) << 1) | (i32::from(automatic) << 2),
            )),
            Operation::Wrap => {
                *wrapped = true;
                Some(PeripheralResponse::number(1))
            }
            Operation::Call(_, _, _) | Operation::CallString(_, _, _) if !callable => {
                return (-2, PeripheralResponse::default())
            }
            Operation::Call(method, arg0, arg1) => device.invoke(method, arg0, arg1),
            Operation::CallString(method, text, arg1) => device.invoke_string(method, &text, arg1),
            Operation::Scan => unreachable!(),
        };
        match response {
            Some(response) if response.text.encode_utf16().count() <= MAX_TEXT_RESULT => {
                (2, response)
            }
            Some(_) => (-4, PeripheralResponse::default()),
            None => (-3, PeripheralResponse::default()),
        }
    }
}
