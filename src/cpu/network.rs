use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

pub(super) const MAX_TEXT: usize = 2048;
pub(super) const MAX_TRANSFER: usize = 65_507;
const TIMEOUT: Duration = Duration::from_secs(5);

pub(super) enum Request {
    Open(i32, String, i32),
    Send(i32, Vec<u8>),
    Receive(i32, i32),
    Close(i32),
    Http(i32, String, Vec<u8>),
}

pub(super) struct Response {
    pub status: i32,
    pub value: i32,
    pub data: Vec<u8>,
}

impl Response {
    fn error(status: i32) -> Self {
        Self {
            status,
            value: 0,
            data: Vec::new(),
        }
    }
}

enum Connection {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

type Connections = HashMap<i32, Connection>;

struct WorkerSlot(Arc<AtomicBool>);

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Default)]
pub struct NetworkPort {
    worker_active: Arc<AtomicBool>,
    allowed: HashSet<(String, u16)>,
    connections: Connections,
    pending: Option<Receiver<(Connections, Response)>>,
    result: Option<Response>,
}

impl NetworkPort {
    /// Authorize a guest to contact one exact host and port.
    pub fn allow(&mut self, host: &str, port: u16) {
        self.allowed.insert((normalize(host), port));
    }

    pub(super) fn reset(&mut self) {
        self.pending = None;
        self.result = None;
        self.connections.clear();
    }

    pub(super) fn service(&mut self) {
        let Some(pending) = &self.pending else { return };
        match pending.try_recv() {
            Ok((connections, response)) => {
                self.connections = connections;
                self.result = Some(response);
            }
            Err(TryRecvError::Disconnected) => self.result = Some(Response::error(-3)),
            Err(TryRecvError::Empty) => return,
        }
        self.pending = None;
    }

    pub(super) fn submit(&mut self, request: Request) {
        if self.allowed.is_empty() {
            self.result = Some(Response::error(-1));
            return;
        }
        // A reset can discard a request while its socket operation is still finishing.
        if self.worker_active.swap(true, Ordering::Acquire) {
            self.result = Some(Response::error(-4));
            return;
        }
        let slot = WorkerSlot(self.worker_active.clone());
        let (sender, receiver) = mpsc::channel();
        let allowed = self.allowed.clone();
        let mut connections = std::mem::take(&mut self.connections);
        match std::thread::Builder::new()
            .name("siliconcraft-network".into())
            .spawn(move || {
                let response = execute(request, &allowed, &mut connections);
                drop(slot);
                let _ = sender.send((connections, response));
            }) {
            Ok(_) => self.pending = Some(receiver),
            Err(_) => self.result = Some(Response::error(-4)),
        }
    }

    pub(super) fn status(&self) -> i32 {
        self.result
            .as_ref()
            .map_or(i32::from(self.pending.is_some()), |result| result.status)
    }

    pub(super) fn result_length(&self) -> i32 {
        self.result
            .as_ref()
            .map_or(0, |result| result.data.len() as i32)
    }

    pub(super) fn take_result(&mut self) -> Option<Response> {
        self.result.take()
    }
}

fn normalize(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn io_status(error: io::Error) -> i32 {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => -6,
        _ => -3,
    }
}

fn execute(
    request: Request,
    allowed: &HashSet<(String, u16)>,
    connections: &mut Connections,
) -> Response {
    let socket = match &request {
        Request::Send(handle, _) | Request::Receive(handle, _) => Some(*handle),
        _ => None,
    };
    match perform(request, allowed, connections) {
        Ok((value, data)) => Response {
            status: 2,
            value,
            data,
        },
        Err(status) => {
            if let (-6, Some(handle)) = (status, socket) {
                connections.remove(&handle);
            }
            Response::error(status)
        }
    }
}

fn perform(
    request: Request,
    allowed: &HashSet<(String, u16)>,
    connections: &mut Connections,
) -> Result<(i32, Vec<u8>), i32> {
    let value = match request {
        Request::Open(protocol, host, port) => {
            if !matches!(protocol, 1 | 2) {
                return Err(-5);
            }
            let handle = (1..=4)
                .find(|handle| !connections.contains_key(handle))
                .ok_or(-4)?;
            let endpoint = (
                normalize(&host),
                u16::try_from(port).ok().filter(|&p| p > 0).ok_or(-5)?,
            );
            if !allowed.contains(&endpoint) {
                return Err(-2);
            }
            let address = endpoint
                .to_socket_addrs()
                .map_err(io_status)?
                .next()
                .ok_or(-3)?;
            let connection = if protocol == 1 {
                let socket = TcpStream::connect_timeout(&address, TIMEOUT).map_err(io_status)?;
                socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
                socket.set_write_timeout(Some(TIMEOUT)).map_err(io_status)?;
                Connection::Tcp(socket)
            } else {
                let socket = UdpSocket::bind(if address.is_ipv4() {
                    "0.0.0.0:0"
                } else {
                    "[::]:0"
                })
                .map_err(io_status)?;
                socket.connect(address).map_err(io_status)?;
                socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
                socket.set_write_timeout(Some(TIMEOUT)).map_err(io_status)?;
                Connection::Udp(socket)
            };
            connections.insert(handle, connection);
            handle
        }
        Request::Send(handle, data) => match connections.get_mut(&handle).ok_or(-5)? {
            Connection::Tcp(socket) => {
                socket.write_all(&data).map_err(io_status)?;
                data.len() as i32
            }
            Connection::Udp(socket) => socket.send(&data).map_err(io_status)? as i32,
        },
        Request::Receive(handle, count) => {
            let count = usize::try_from(count)
                .ok()
                .filter(|n| (1..=MAX_TRANSFER).contains(n))
                .ok_or(-5)?;
            let mut data = vec![0; count];
            let connection = connections.get_mut(&handle).ok_or(-5)?;
            let count = match connection {
                Connection::Tcp(socket) => socket.read(&mut data),
                Connection::Udp(socket) => socket.recv(&mut data),
            }
            .map_err(io_status)?;
            data.truncate(count);
            let value = if count == 0 && matches!(connection, Connection::Tcp(_)) {
                -1
            } else {
                count as i32
            };
            return Ok((value, data));
        }
        Request::Close(handle) => {
            connections.remove(&handle).ok_or(-5)?;
            1
        }
        Request::Http(method, url, data) => return http(method, &url, data, allowed),
    };
    Ok((value, Vec::new()))
}

fn http(
    method: i32,
    url: &str,
    data: Vec<u8>,
    allowed: &HashSet<(String, u16)>,
) -> Result<(i32, Vec<u8>), i32> {
    use reqwest::{blocking::Client, Method, Url};

    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::HEAD,
    ];
    let method = methods.get(method as usize).ok_or(-5)?.clone();
    let url = Url::parse(url).map_err(|_| -5)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(-5);
    }
    let endpoint = (
        normalize(url.host_str().ok_or(-5)?),
        url.port_or_known_default().ok_or(-5)?,
    );
    if !allowed.contains(&endpoint) {
        return Err(-2);
    }
    let client = Client::builder()
        .timeout(TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| -3)?;
    let response = client
        .request(method, url)
        .body(data)
        .send()
        .map_err(|error| if error.is_timeout() { -6 } else { -3 })?;
    let status = i32::from(response.status().as_u16());
    let mut data = Vec::new();
    response
        .take(MAX_TRANSFER as u64)
        .read_to_end(&mut data)
        .map_err(io_status)?;
    Ok((status, data))
}
