use hidmaestro_client::{Buttons, State};
use std::io::{self, Read, Write};

const FRAME: usize = 14;

pub(super) fn configure_stream(stream: &std::net::TcpStream) -> io::Result<()> {
    // On Windows accepted sockets inherit the listener's nonblocking mode.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(super::TIMEOUT))?;
    stream.set_write_timeout(Some(super::TIMEOUT))?;
    stream.set_nodelay(true)
}

pub(super) fn encode(state: State) -> [u8; FRAME] {
    let mut bytes = [0; FRAME];
    bytes[..4].copy_from_slice(&state.buttons.bits().to_le_bytes());
    for (i, axis) in [state.left_x, state.left_y, state.right_x, state.right_y].into_iter().enumerate() {
        bytes[4 + i * 2..6 + i * 2].copy_from_slice(&axis.to_le_bytes());
    }
    bytes[12] = state.left_trigger;
    bytes[13] = state.right_trigger;
    bytes
}

pub(super) fn decode(bytes: [u8; FRAME]) -> io::Result<State> {
    let bits = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    let buttons = Buttons::from_bits(bits)
        .filter(|_| bits & 3 != 3 && bits & 12 != 12)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid gamepad buttons"))?;
    let axis = |i| i16::from_le_bytes([bytes[i], bytes[i + 1]]);
    Ok(State {
        buttons,
        left_x: axis(4),
        left_y: axis(6),
        right_x: axis(8),
        right_y: axis(10),
        left_trigger: bytes[12],
        right_trigger: bytes[13],
    })
}

pub(super) fn serve(mut stream: impl Read + Write, mut submit: impl FnMut(State) -> io::Result<()>) -> io::Result<()> {
    loop {
        let mut bytes = [0; FRAME];
        if stream.read(&mut bytes[..1])? == 0 {
            return Ok(());
        }
        stream.read_exact(&mut bytes[1..])?;
        submit(decode(bytes)?)?;
        stream.write_all(&[0])?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    #[test]
    fn accepted_socket_waits_for_delayed_startup_response() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        configure_stream(&server).unwrap();
        let task = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            client.write_all(&[42]).unwrap();
        });
        let mut reply = [0];
        let result = server.read_exact(&mut reply);
        task.join().unwrap();
        result.unwrap();
        assert_eq!(reply, [42]);
    }

    #[test]
    fn wire_preserves_axes_triggers_and_buttons() {
        let state = State {
            buttons: Buttons::A | Buttons::BACK,
            left_x: -32768,
            left_y: 32767,
            right_x: -123,
            right_y: 456,
            left_trigger: 128,
            right_trigger: 255,
        };
        assert_eq!(encode(state), [0x20, 0x10, 0, 0, 0, 128, 255, 127, 133, 255, 200, 1, 128, 255]);
        assert_eq!(decode(encode(state)).unwrap(), state);
    }

    #[test]
    fn invalid_buttons_never_reach_submit() {
        for bits in [0x80000000u32, 3, 12] {
            let mut frame = [0; FRAME];
            frame[..4].copy_from_slice(&bits.to_le_bytes());
            assert!(decode(frame).is_err());
        }
    }

    #[test]
    fn frames_are_acknowledged_and_eof_ends_session() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let (server, _) = listener.accept().unwrap();
        let task = std::thread::spawn(move || {
            let mut seen = Vec::new();
            serve(server, |s| {
                seen.push(s);
                Ok(())
            })
            .unwrap();
            seen
        });
        client.write_all(&[0; FRAME]).unwrap();
        let mut ack = [255];
        client.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [0]);
        client.shutdown(Shutdown::Both).unwrap();
        assert_eq!(task.join().unwrap(), [State::default()]);
    }

    #[test]
    fn stalled_parent_expires_without_submitting() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        server.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let started = Instant::now();
        let error = serve(server, |_| panic!("no complete frame")).unwrap_err();
        assert!(matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn partial_frame_is_rejected() {
        assert_eq!(serve(&mut io::Cursor::new(vec![1]), |_| panic!("partial frame")).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
