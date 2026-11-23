use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

pub(super) const MAX_TEXT: usize = 2048;
pub(super) const MAX_TRANSFER: usize = 65_507;
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HANDLES: usize = 4;

const TCP: i32 = 1;
const UDP: i32 = 2;
const SMTPS: i32 = 4;
const IMAPS: i32 = 6;
const HTTP: i32 = 7;
const HTTPS: i32 = 8;
const WS: i32 = 9;
const WSS: i32 = 10;

const DISABLED: i32 = -1;
const DENIED: i32 = -2;
const IO_ERROR: i32 = -3;
const LIMIT: i32 = -4;
const INVALID: i32 = -5;
const TIMED_OUT: i32 = -6;

pub(super) enum Request {
    Open(i32, String, i32),
    /// Handle, payload, UTF-8 text message.
    Send(i32, Vec<u8>, bool),
    Receive(i32, i32),
    Close(i32),
    /// Method, URL, body, plain-text content type, HTTP/3.
    Http(i32, String, Vec<u8>, bool, bool),
    WebSocket(String),
    Tls(i32),
    Listen(i32, i32),
    Accept(i32),
    /// Handle, status, body, plain-text content type.
    Respond(i32, i32, Vec<u8>, bool),
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

/// A TCP stream, optionally wrapped in TLS.
enum Transport {
    Plain(TcpStream),
    Client(Box<StreamOwned<ClientConnection, TcpStream>>),
    Server(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Transport {
    fn finish(&mut self) {
        match self {
            Transport::Plain(_) => {}
            Transport::Client(stream) => {
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
            Transport::Server(stream) => {
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
        }
    }
}

impl Read for Transport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(stream) => stream.read(buffer),
            Transport::Client(stream) => stream.read(buffer),
            Transport::Server(stream) => stream.read(buffer),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(stream) => stream.write(buffer),
            Transport::Client(stream) => stream.write(buffer),
            Transport::Server(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Transport::Plain(stream) => stream.flush(),
            Transport::Client(stream) => stream.flush(),
            Transport::Server(stream) => stream.flush(),
        }
    }
}

enum Connection {
    /// Raw stream. The peer is remembered for explicit `NTLS` upgrades.
    Stream(Transport, Option<(String, u16)>),
    Udp(UdpSocket),
    WebSocket(Box<WebSocket<Transport>>),
    /// An accepted HTTP request awaiting its response.
    Http(Transport, String),
}

enum Listener {
    Stream(TcpListener, i32),
    Datagram(UdpSocket, Option<SocketAddr>),
}

#[derive(Default)]
struct Sockets {
    connections: HashMap<i32, Connection>,
    listeners: HashMap<i32, Listener>,
}

impl Sockets {
    fn next_handle(&self) -> i32 {
        (1..)
            .find(|handle| {
                !self.connections.contains_key(handle) && !self.listeners.contains_key(handle)
            })
            .unwrap_or(1)
    }
}

struct WorkerSlot(Arc<AtomicBool>);

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone, Default)]
struct Policy {
    allowed: HashSet<(String, u16)>,
    listen: HashSet<u16>,
    server_tls: Option<Arc<ServerConfig>>,
}

#[derive(Default)]
pub struct NetworkPort {
    worker_active: Arc<AtomicBool>,
    policy: Policy,
    sockets: Sockets,
    pending: Option<Receiver<(Sockets, Response)>>,
    result: Option<Response>,
}

impl NetworkPort {
    /// Authorize a guest to contact one exact host and port.
    pub fn allow(&mut self, host: &str, port: u16) {
        self.policy.allowed.insert((normalize(host), port));
    }

    /// Authorize a guest to open a listener on one local port.
    pub fn allow_listen(&mut self, port: u16) {
        self.policy.listen.insert(port);
    }

    /// Load the PEM certificate chain and private key used by HTTPS and WSS listeners.
    pub fn set_server_tls(&mut self, certificate: &Path, key: &Path) -> Result<(), String> {
        let chain = CertificateDer::pem_file_iter(certificate)
            .map_err(|error| format!("{}: {error}", certificate.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("{}: {error}", certificate.display()))?;
        let key = PrivateKeyDer::from_pem_file(key)
            .map_err(|error| format!("{}: {error}", key.display()))?;
        let config = ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|error| error.to_string())?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|error| error.to_string())?;
        self.policy.server_tls = Some(Arc::new(config));
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.pending = None;
        self.result = None;
        self.sockets = Sockets::default();
    }

    pub(super) fn service(&mut self) {
        let Some(pending) = &self.pending else { return };
        match pending.try_recv() {
            Ok((sockets, response)) => {
                self.sockets = sockets;
                self.result = Some(response);
            }
            Err(TryRecvError::Disconnected) => self.result = Some(Response::error(IO_ERROR)),
            Err(TryRecvError::Empty) => return,
        }
        self.pending = None;
    }

    /// True when no request is outstanding and no result waits to be consumed.
    pub(super) fn ready(&self) -> bool {
        self.pending.is_none() && self.result.is_none()
    }

    pub(super) fn submit(&mut self, request: Request) {
        if self.policy.allowed.is_empty() && self.policy.listen.is_empty() {
            self.result = Some(Response::error(DISABLED));
            return;
        }
        // A reset can discard a request while its socket operation is still finishing.
        if self.worker_active.swap(true, Ordering::Acquire) {
            self.result = Some(Response::error(LIMIT));
            return;
        }
        let slot = WorkerSlot(self.worker_active.clone());
        let (sender, receiver) = mpsc::channel();
        let policy = self.policy.clone();
        let mut sockets = std::mem::take(&mut self.sockets);
        match std::thread::Builder::new()
            .name("siliconcraft-network".into())
            .spawn(move || {
                let response = execute(request, &policy, &mut sockets);
                drop(slot);
                let _ = sender.send((sockets, response));
            }) {
            Ok(_) => self.pending = Some(receiver),
            Err(_) => self.result = Some(Response::error(LIMIT)),
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
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => TIMED_OUT,
        _ => IO_ERROR,
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let config = ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("TLS protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

fn secure_client(socket: TcpStream, host: &str) -> Result<Transport, i32> {
    let name = ServerName::try_from(host.to_owned()).map_err(|_| INVALID)?;
    let mut connection = ClientConnection::new(client_config(), name).map_err(|_| IO_ERROR)?;
    let mut socket = socket;
    // Complete the handshake now so certificate errors surface on the request that caused them.
    while connection.is_handshaking() {
        connection.complete_io(&mut socket).map_err(io_status)?;
    }
    Ok(Transport::Client(Box::new(StreamOwned::new(connection, socket))))
}

fn secure_server(socket: TcpStream, config: &Arc<ServerConfig>) -> Result<Transport, i32> {
    let mut connection = ServerConnection::new(config.clone()).map_err(|_| IO_ERROR)?;
    let mut socket = socket;
    while connection.is_handshaking() {
        connection.complete_io(&mut socket).map_err(io_status)?;
    }
    Ok(Transport::Server(Box::new(StreamOwned::new(connection, socket))))
}

fn connect(host: &str, port: u16) -> Result<TcpStream, i32> {
    let address = (host, port)
        .to_socket_addrs()
        .map_err(io_status)?
        .next()
        .ok_or(IO_ERROR)?;
    let socket = TcpStream::connect_timeout(&address, TIMEOUT).map_err(io_status)?;
    socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
    socket.set_write_timeout(Some(TIMEOUT)).map_err(io_status)?;
    Ok(socket)
}

fn execute(request: Request, policy: &Policy, sockets: &mut Sockets) -> Response {
    let socket = match &request {
        Request::Send(handle, ..) | Request::Receive(handle, _) => Some(*handle),
        _ => None,
    };
    match perform(request, policy, sockets) {
        Ok((value, data)) => Response {
            status: 2,
            value,
            data,
        },
        Err(status) => {
            if let (TIMED_OUT, Some(handle)) = (status, socket) {
                sockets.connections.remove(&handle);
            }
            Response::error(status)
        }
    }
}

type Outcome = Result<(i32, Vec<u8>), i32>;

fn perform(request: Request, policy: &Policy, sockets: &mut Sockets) -> Outcome {
    match request {
        Request::Open(protocol, host, port) => open(protocol, &host, port, policy, sockets),
        Request::WebSocket(url) => open_websocket(&url, policy, sockets),
        Request::Send(handle, data, text) => send(handle, data, text, sockets),
        Request::Receive(handle, count) => receive(handle, count, sockets),
        Request::Close(handle) => close(handle, sockets),
        Request::Http(method, url, data, text, http3) => {
            http(method, &url, data, text, http3, &policy.allowed)
        }
        Request::Tls(handle) => upgrade_tls(handle, sockets),
        Request::Listen(protocol, port) => listen(protocol, port, policy, sockets),
        Request::Accept(handle) => accept(handle, policy, sockets),
        Request::Respond(handle, status, data, text) => respond(handle, status, &data, text, sockets),
    }
}

fn reserve_connection(sockets: &Sockets) -> Result<i32, i32> {
    if sockets.connections.len() >= MAX_HANDLES {
        return Err(LIMIT);
    }
    Ok(sockets.next_handle())
}

fn open(protocol: i32, host: &str, port: i32, policy: &Policy, sockets: &mut Sockets) -> Outcome {
    if !(TCP..=IMAPS).contains(&protocol) {
        return Err(INVALID);
    }
    let handle = reserve_connection(sockets)?;
    let host = normalize(host);
    let port = u16::try_from(port).ok().filter(|&p| p > 0).ok_or(INVALID)?;
    if !policy.allowed.contains(&(host.clone(), port)) {
        return Err(DENIED);
    }
    let connection = if protocol == UDP {
        let address = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(io_status)?
            .next()
            .ok_or(IO_ERROR)?;
        let socket = UdpSocket::bind(if address.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })
            .map_err(io_status)?;
        socket.connect(address).map_err(io_status)?;
        socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
        socket.set_write_timeout(Some(TIMEOUT)).map_err(io_status)?;
        Connection::Udp(socket)
    } else {
        let socket = connect(&host, port)?;
        let transport = if matches!(protocol, SMTPS | IMAPS) {
            secure_client(socket, &host)?
        } else {
            Transport::Plain(socket)
        };
        Connection::Stream(transport, Some((host, port)))
    };
    sockets.connections.insert(handle, connection);
    Ok((handle, Vec::new()))
}

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_TRANSFER))
        .max_frame_size(Some(MAX_TRANSFER))
}

fn websocket_error(error: tungstenite::Error) -> i32 {
    match error {
        tungstenite::Error::Io(error) => io_status(error),
        tungstenite::Error::Capacity(_) => LIMIT,
        tungstenite::Error::Url(_) => INVALID,
        _ => IO_ERROR,
    }
}

fn open_websocket(url: &str, policy: &Policy, sockets: &mut Sockets) -> Outcome {
    let url = reqwest::Url::parse(url).map_err(|_| INVALID)?;
    let tls = match url.scheme() {
        "wss" => true,
        "ws" => false,
        _ => return Err(INVALID),
    };
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(INVALID);
    }
    let handle = reserve_connection(sockets)?;
    let host = normalize(url.host_str().ok_or(INVALID)?);
    let port = url.port_or_known_default().ok_or(INVALID)?;
    if !policy.allowed.contains(&(host.clone(), port)) {
        return Err(DENIED);
    }
    let socket = connect(&host, port)?;
    let transport = if tls { secure_client(socket, &host)? } else { Transport::Plain(socket) };
    let (socket, _) = tungstenite::client::client_with_config(url.as_str(), transport, Some(websocket_config()))
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => websocket_error(error),
            tungstenite::HandshakeError::Interrupted(_) => TIMED_OUT,
        })?;
    sockets.connections.insert(handle, Connection::WebSocket(Box::new(socket)));
    Ok((handle, Vec::new()))
}

fn send(handle: i32, data: Vec<u8>, text: bool, sockets: &mut Sockets) -> Outcome {
    if let Some(listener) = sockets.listeners.get(&handle) {
        let Listener::Datagram(socket, Some(peer)) = listener else {
            return Err(INVALID);
        };
        if text {
            return Err(INVALID);
        }
        let sent = socket.send_to(&data, peer).map_err(io_status)?;
        return Ok((sent as i32, Vec::new()));
    }
    let length = data.len() as i32;
    match sockets.connections.get_mut(&handle).ok_or(INVALID)? {
        Connection::Stream(_, _) | Connection::Udp(_) if text => return Err(INVALID),
        Connection::Stream(transport, _) => {
            transport.write_all(&data).map_err(io_status)?;
            transport.flush().map_err(io_status)?;
        }
        Connection::Udp(socket) => return Ok((socket.send(&data).map_err(io_status)? as i32, Vec::new())),
        Connection::WebSocket(socket) => {
            let message = if text {
                Message::text(String::from_utf8(data).map_err(|_| INVALID)?)
            } else {
                Message::binary(data)
            };
            if let Err(error) = socket.send(message) {
                let status = websocket_error(error);
                if status != TIMED_OUT {
                    sockets.connections.remove(&handle);
                }
                return Err(status);
            }
        }
        Connection::Http(..) => return Err(INVALID),
    }
    Ok((length, Vec::new()))
}

fn receive(handle: i32, count: i32, sockets: &mut Sockets) -> Outcome {
    let count = usize::try_from(count)
        .ok()
        .filter(|n| (1..=MAX_TRANSFER).contains(n))
        .ok_or(INVALID)?;
    if let Some(listener) = sockets.listeners.get_mut(&handle) {
        let Listener::Datagram(socket, peer) = listener else {
            return Err(INVALID);
        };
        socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
        let mut data = vec![0; count + 1];
        let (length, sender) = socket.recv_from(&mut data).map_err(io_status)?;
        if length > count {
            return Err(LIMIT);
        }
        *peer = Some(sender);
        data.truncate(length);
        return Ok((length as i32, data));
    }
    let connection = sockets.connections.get_mut(&handle).ok_or(INVALID)?;
    match connection {
        Connection::Http(..) => Err(INVALID),
        Connection::WebSocket(socket) => {
            let outcome = loop {
                match socket.read() {
                    Ok(Message::Text(text)) => break Ok(text.as_bytes().to_vec()),
                    Ok(Message::Binary(bytes)) => break Ok(bytes.to_vec()),
                    Ok(Message::Close(_))
                    | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                        sockets.connections.remove(&handle);
                        return Ok((-1, Vec::new()));
                    }
                    Ok(_) => continue,
                    Err(error) => break Err(websocket_error(error)),
                }
            };
            match outcome {
                Ok(message) if message.len() <= count => Ok((message.len() as i32, message)),
                Ok(_) => {
                    sockets.connections.remove(&handle);
                    Err(LIMIT)
                }
                Err(status) => {
                    if status != TIMED_OUT {
                        sockets.connections.remove(&handle);
                    }
                    Err(status)
                }
            }
        }
        Connection::Stream(transport, _) => {
            let mut data = vec![0; count];
            let length = match transport.read(&mut data) {
                Ok(length) => length,
                // Peers that vanish without a TLS close_notify are treated as end of stream.
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => 0,
                Err(error) => return Err(io_status(error)),
            };
            data.truncate(length);
            Ok((if length == 0 { -1 } else { length as i32 }, data))
        }
        Connection::Udp(socket) => {
            let mut data = vec![0; count];
            let length = socket.recv(&mut data).map_err(io_status)?;
            data.truncate(length);
            Ok((length as i32, data))
        }
    }
}

fn close(handle: i32, sockets: &mut Sockets) -> Outcome {
    if sockets.listeners.remove(&handle).is_some() {
        return Ok((1, Vec::new()));
    }
    match sockets.connections.remove(&handle).ok_or(INVALID)? {
        Connection::Stream(mut transport, _) | Connection::Http(mut transport, _) => transport.finish(),
        Connection::WebSocket(mut socket) => {
            let _ = socket.close(None);
            let _ = socket.flush();
        }
        Connection::Udp(_) => {}
    }
    Ok((1, Vec::new()))
}

fn upgrade_tls(handle: i32, sockets: &mut Sockets) -> Outcome {
    match sockets.connections.remove(&handle).ok_or(INVALID)? {
        Connection::Stream(Transport::Plain(socket), Some((host, port))) => {
            let transport = secure_client(socket, &host)?;
            sockets.connections.insert(handle, Connection::Stream(transport, Some((host, port))));
            Ok((1, Vec::new()))
        }
        other => {
            sockets.connections.insert(handle, other);
            Err(INVALID)
        }
    }
}

fn listen(protocol: i32, port: i32, policy: &Policy, sockets: &mut Sockets) -> Outcome {
    let port = u16::try_from(port).ok().filter(|&p| p > 0).ok_or(INVALID)?;
    if !matches!(protocol, TCP | UDP | HTTP | HTTPS | WS | WSS) {
        return Err(INVALID);
    }
    if !policy.listen.contains(&port) {
        return Err(DENIED);
    }
    if sockets.listeners.len() >= MAX_HANDLES {
        return Err(LIMIT);
    }
    let secure = matches!(protocol, HTTPS | WSS);
    if secure && policy.server_tls.is_none() {
        return Err(IO_ERROR);
    }
    let listener = if protocol == UDP {
        Listener::Datagram(UdpSocket::bind(("0.0.0.0", port)).map_err(io_status)?, None)
    } else {
        Listener::Stream(TcpListener::bind(("0.0.0.0", port)).map_err(io_status)?, protocol)
    };
    let handle = sockets.next_handle();
    sockets.listeners.insert(handle, listener);
    Ok((handle, Vec::new()))
}

fn accept(handle: i32, policy: &Policy, sockets: &mut Sockets) -> Outcome {
    let connection_handle = reserve_connection(sockets)?;
    let Some(Listener::Stream(listener, protocol)) = sockets.listeners.get(&handle) else {
        return Err(INVALID);
    };
    let protocol = *protocol;
    listener.set_nonblocking(true).map_err(io_status)?;
    let deadline = Instant::now() + TIMEOUT;
    let socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(TIMED_OUT);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(io_status(error)),
        }
    };
    socket.set_nonblocking(false).map_err(io_status)?;
    socket.set_read_timeout(Some(TIMEOUT)).map_err(io_status)?;
    socket.set_write_timeout(Some(TIMEOUT)).map_err(io_status)?;
    let mut transport = if matches!(protocol, HTTPS | WSS) {
        secure_server(socket, policy.server_tls.as_ref().ok_or(IO_ERROR)?)?
    } else {
        Transport::Plain(socket)
    };
    let (connection, data) = match protocol {
        TCP => (Connection::Stream(transport, None), Vec::new()),
        HTTP | HTTPS => {
            let (method, bytes) = read_http(&mut transport)?;
            (Connection::Http(transport, method), bytes)
        }
        _ => {
            let mut request_bytes = Vec::new();
            let accepted = tungstenite::accept_hdr_with_config(
                transport,
                |request: &tungstenite::handshake::server::Request,
                 response: tungstenite::handshake::server::Response| {
                    let target = request
                        .uri()
                        .path_and_query()
                        .map_or("/", |target| target.as_str());
                    request_bytes.extend(format!("{} {target} HTTP/1.1\r\n", request.method()).bytes());
                    for (name, value) in request.headers() {
                        request_bytes.extend(name.as_str().bytes());
                        request_bytes.extend(b": ");
                        request_bytes.extend(value.as_bytes());
                        request_bytes.extend(b"\r\n");
                    }
                    request_bytes.extend(b"\r\n");
                    Ok(response)
                },
                Some(websocket_config()),
            )
            .map_err(|_| IO_ERROR)?;
            if request_bytes.len() > MAX_TRANSFER {
                return Err(LIMIT);
            }
            (Connection::WebSocket(Box::new(accepted)), request_bytes)
        }
    };
    sockets.connections.insert(connection_handle, connection);
    Ok((connection_handle, data))
}

fn read_line(reader: &mut impl BufRead) -> Result<String, i32> {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        reader.read_exact(&mut byte).map_err(io_status)?;
        match byte[0] {
            b'\r' => {
                reader.read_exact(&mut byte).map_err(io_status)?;
                if byte[0] != b'\n' {
                    return Err(IO_ERROR);
                }
                return Ok(line.into_iter().map(char::from).collect());
            }
            b'\n' => return Err(IO_ERROR),
            value => line.push(value),
        }
        if line.len() > 8192 {
            return Err(IO_ERROR);
        }
    }
}

/// Read one HTTP/1.0 or HTTP/1.1 request, returning its method and raw bytes.
fn read_http(transport: &mut Transport) -> Result<(String, Vec<u8>), i32> {
    let mut reader = BufReader::new(transport);
    let mut bytes = Vec::new();
    let append = |bytes: &mut Vec<u8>, line: &str| -> Result<(), i32> {
        if line.len() + 2 > MAX_TRANSFER - bytes.len() {
            return Err(LIMIT);
        }
        bytes.extend(line.bytes());
        bytes.extend(b"\r\n");
        Ok(())
    };
    let request_line = read_line(&mut reader)?;
    let parts: Vec<&str> = request_line.split(' ').collect();
    let token = |text: &str| {
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    };
    if parts.len() != 3
        || !token(parts[0])
        || !parts[1].starts_with('/')
        || !parts[1].bytes().all(|b| (b'!'..=b'~').contains(&b))
        || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1")
    {
        return Err(IO_ERROR);
    }
    append(&mut bytes, &request_line)?;
    let mut headers: HashMap<String, String> = HashMap::new();
    for index in 0.. {
        let line = read_line(&mut reader)?;
        append(&mut bytes, &line)?;
        if line.is_empty() {
            break;
        }
        let separator = line.find(':').unwrap_or(0);
        if index >= 64 || separator < 1 || !token(&line[..separator]) {
            return Err(IO_ERROR);
        }
        let value = &line[separator + 1..];
        if value.bytes().any(|b| (b < 32 && b != b'\t') || b == 127) {
            return Err(IO_ERROR);
        }
        headers
            .entry(line[..separator].to_ascii_lowercase())
            .and_modify(|first| {
                first.push(',');
                first.push_str(value.trim());
            })
            .or_insert_with(|| value.trim().to_string());
    }
    if parts[2] == "HTTP/1.1"
        && !headers
            .get("host")
            .is_some_and(|host| !host.is_empty() && !host.contains(|c: char| c == ',' || c.is_whitespace()))
    {
        return Err(IO_ERROR);
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err(IO_ERROR);
    }
    let length = headers.get("content-length").map_or("0", String::as_str);
    if length.is_empty() || length.len() > 18 || !length.bytes().all(|b| b.is_ascii_digit()) {
        return Err(IO_ERROR);
    }
    let length: usize = length.parse().map_err(|_| IO_ERROR)?;
    if length > MAX_TRANSFER - bytes.len() {
        return Err(LIMIT);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(io_status)?;
    bytes.extend(body);
    Ok((parts[0].to_string(), bytes))
}

fn respond(handle: i32, status: i32, data: &[u8], text: bool, sockets: &mut Sockets) -> Outcome {
    if !(200..=599).contains(&status) {
        return Err(INVALID);
    }
    if data.len() > MAX_TRANSFER {
        return Err(LIMIT);
    }
    if !matches!(sockets.connections.get(&handle), Some(Connection::Http(..))) {
        return Err(INVALID);
    }
    let Some(Connection::Http(mut transport, method)) = sockets.connections.remove(&handle) else {
        return Err(INVALID);
    };
    let no_body = matches!(status, 204 | 205 | 304);
    let length = if matches!(status, 204 | 304) {
        String::new()
    } else {
        format!("Content-Length: {}\r\n", if no_body { 0 } else { data.len() })
    };
    let content_type = if text { "text/plain; charset=utf-8" } else { "application/octet-stream" };
    let header = format!(
        "HTTP/1.1 {status} \r\nConnection: close\r\nContent-Type: {content_type}\r\n{length}\r\n"
    );
    transport.write_all(header.as_bytes()).map_err(io_status)?;
    if !no_body && method != "HEAD" {
        transport.write_all(data).map_err(io_status)?;
    }
    transport.flush().map_err(io_status)?;
    transport.finish();
    Ok((1, Vec::new()))
}

fn http(
    method: i32,
    url: &str,
    data: Vec<u8>,
    text: bool,
    http3: bool,
    allowed: &HashSet<(String, u16)>,
) -> Outcome {
    use reqwest::{blocking::Client, Method, Url};

    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::HEAD,
    ];
    let method = usize::try_from(method)
        .ok()
        .and_then(|index| methods.get(index))
        .ok_or(INVALID)?
        .clone();
    let url = Url::parse(url).map_err(|_| INVALID)?;
    let scheme_ok = if http3 {
        url.scheme() == "https"
    } else {
        matches!(url.scheme(), "http" | "https")
    };
    if !scheme_ok
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(INVALID);
    }
    let endpoint = (
        normalize(url.host_str().ok_or(INVALID)?),
        url.port_or_known_default().ok_or(INVALID)?,
    );
    if !allowed.contains(&endpoint) {
        return Err(DENIED);
    }
    let mut builder = Client::builder()
        .timeout(TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none());
    if http3 {
        builder = builder.http3_prior_knowledge();
    }
    let client = builder.build().map_err(|_| IO_ERROR)?;
    let mut request = client.request(method, url).body(data);
    if text {
        request = request.header("Content-Type", "text/plain; charset=utf-8");
    }
    let response = request
        .send()
        .map_err(|error| if error.is_timeout() { TIMED_OUT } else { IO_ERROR })?;
    let status = i32::from(response.status().as_u16());
    let mut data = Vec::new();
    response
        .take(MAX_TRANSFER as u64)
        .read_to_end(&mut data)
        .map_err(io_status)?;
    Ok((status, data))
}
