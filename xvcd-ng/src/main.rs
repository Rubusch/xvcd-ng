mod backend;

use backend::{DummyBackend, JtagController};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const DEFAULT_PORT: &str = "2542";
const XVC_INFO_STRING: &[u8] = b"xvcServer:v1.0\n";

fn handle_client<T: JtagController>(mut stream: TcpStream, hardware: &mut T) -> std::io::Result<()> {
    println!("Client connected: {:?}", stream.peer_addr()?);
    let mut buffer = [0u8; 8];

    loop {
        if stream.read_exact(&mut buffer).is_err() {
            break;
        }

        if &buffer[0..8] == b"getinfo:" {
            stream.write_all(XVC_INFO_STRING)?;
            stream.flush()?;
        } else if &buffer[0..8] == b"settck:" {
            let mut period_buf = [0u8; 4];
            stream.read_exact(&mut period_buf)?;
            let requested_ns = (&period_buf[..]).read_u32::<LittleEndian>()?;
            let tck_period_ns = hardware.set_tck_period(requested_ns);
            let mut reply = [0u8; 4];
            (&mut reply[..]).write_u32::<LittleEndian>(tck_period_ns)?;
            stream.write_all(&reply)?;
            stream.flush()?;
        } else if &buffer[0..6] == b"shift:" {
            let b0 = buffer[6] as u32;
            let b1 = buffer[7] as u32;
            let b2 = stream.read_u16::<LittleEndian>()? as u32;
            let num_bits = b0 | (b1 << 8) | (b2 << 16);

            let byte_len = ((num_bits + 7) / 8) as usize;

            let mut tms_bytes = vec![0u8; byte_len];
            let mut tdi_bytes = vec![0u8; byte_len];
            let mut tdo_bytes = vec![0u8; byte_len];

            stream.read_exact(&mut tms_bytes)?;
            stream.read_exact(&mut tdi_bytes)?;

            hardware.shift(num_bits, &tms_bytes, &tdi_bytes, &mut tdo_bytes);

            stream.write_all(&tdo_bytes)?;
            stream.flush()?;
        } else {
            eprintln!("Unknown command protocol fragment received.");
            break;
        }
    }
    println!("Client disconnected.");
    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", DEFAULT_PORT))?;
    println!("xvcd-ng listening on port {}...", DEFAULT_PORT);

    let mut hardware = DummyBackend;
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_client(s, &mut hardware) {
                    eprintln!("Error handling connection session: {}", e);
                }
            }
            Err(e) => eprintln!("Connection intake failure: {}", e),
        }
    }
    Ok(())
}
