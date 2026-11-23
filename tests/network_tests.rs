use siliconcraft_emulator::{State, VirtualCpu};
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

fn run(cpu: &mut VirtualCpu, program: &[String]) {
    cpu.burn_to_rom(program);
    let deadline = Instant::now() + Duration::from_secs(6);
    while cpu.state == State::Running && Instant::now() < deadline {
        cpu.execute_cycles(100);
        thread::yield_now();
    }
    assert_eq!(cpu.state, State::Halted, "{:?}", cpu.terminal_output);
}

fn emit_request(
    program: &mut Vec<String>,
    request: String,
    result: &str,
    address: usize,
    length: usize,
) {
    let label = format!("wait_{}", program.len());
    program.extend([
        request,
        format!("{label}:"),
        "NSTATUS B".into(),
        "CMP B, 1".into(),
        format!("JZ {label}"),
        "CMP B, 2".into(),
        "JNZ failed".into(),
        "NLEN C".into(),
        format!("NRESULT {result}, {address}, {length}"),
    ]);
}

fn put_text(program: &mut Vec<String>, address: usize, value: &str) {
    for (offset, byte) in value.bytes().enumerate() {
        program.push(format!("MOV [{}], {byte}", address + offset));
    }
    program.push(format!("MOV [{}], 0", address + value.len()));
}

#[test]
fn guest_can_exchange_bytes_over_tcp_and_udp() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let ports = [
        tcp.local_addr().unwrap().port(),
        udp.local_addr().unwrap().port(),
    ];
    udp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let tcp_server = thread::spawn(move || {
        let (mut stream, _) = tcp.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut data = [0];
        stream.read_exact(&mut data).unwrap();
        assert_eq!(data, [99]);
        stream.write_all(&[201]).unwrap();
    });
    let udp_server = thread::spawn(move || {
        let mut data = [0];
        let (count, client) = udp.recv_from(&mut data).unwrap();
        assert_eq!((count, data), (1, [99]));
        udp.send_to(&[201], client).unwrap();
    });

    for (protocol, port) in (1..=2).zip(ports) {
        let mut cpu = VirtualCpu::default();
        cpu.network_port.allow("127.0.0.1", port);
        let mut program = Vec::new();
        put_text(&mut program, 10, "127.0.0.1");
        program.push("MOV [0], 99".into());
        emit_request(
            &mut program,
            format!("NOPEN A, {protocol}, 10, {port}"),
            "D",
            0,
            0,
        );
        emit_request(&mut program, "NSEND A, D, 0, 1".into(), "[1]", 0, 0);
        emit_request(&mut program, "NRECV A, D, 1".into(), "[2]", 3, 1);
        emit_request(&mut program, "NCLOSE A, D".into(), "[4]", 0, 0);
        program.extend([
            "HLT".into(),
            "failed:".into(),
            "MOV A, -99".into(),
            "HLT".into(),
        ]);
        run(&mut cpu, &program);
        assert_eq!(cpu.reg_a, 1, "network status {}", cpu.reg_b);
        assert_eq!(&cpu.ram.as_slice()[..5], &[99, 1, 1, 201, 1]);
    }
    tcp_server.join().unwrap();
    udp_server.join().unwrap();
}

#[test]
fn guest_http_reports_status_and_copies_response_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        use std::io::BufRead;
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "GET /test HTTP/1.1\r\n");
        while !line.trim().is_empty() {
            line.clear();
            reader.read_line(&mut line).unwrap();
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc")
            .unwrap();
    });
    let mut cpu = VirtualCpu::default();
    cpu.network_port.allow("127.0.0.1", port);
    let mut program = Vec::new();
    put_text(&mut program, 10, &format!("http://127.0.0.1:{port}/test"));
    emit_request(&mut program, "NHTTP A, 0, 10, 0, 0".into(), "D", 0, 2);
    program.extend([
        "HLT".into(),
        "failed:".into(),
        "MOV A, -99".into(),
        "HLT".into(),
    ]);
    run(&mut cpu, &program);
    assert_eq!((cpu.reg_a, cpu.reg_c, cpu.reg_d), (1, 3, 200));
    assert_eq!(&cpu.ram.as_slice()[..3], &[97, 98, 0]);
    server.join().unwrap();
}
