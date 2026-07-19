use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use std::net::{TcpListener};
use log::{info, error};

pub const XVC_INFO_STRING: &[u8] = b"xvcServer:v1.0\n";

pub trait JtagController {
    fn set_tck_period(&mut self, period_ns: u32) -> u32;
    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]);
}

pub struct XvcServer {
    listener: TcpListener,
}

impl XvcServer {
    pub fn new(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        Ok(XvcServer { listener })
    }

    pub fn run(&self, hardware: &mut dyn JtagController) -> std::io::Result<()> {
        info!("xvcd-ng server listening for connections...");
        for stream in self.listener.incoming() {
            match stream {
                Ok(s) => {
                    info!("Client connected: {:?}", s.peer_addr());
                    if let Err(e) = self.process_xvc_stream(s, hardware) {
                        error!("Session network error: {}", e);
                    }
                    info!("Client disconnected.");
                }
                Err(e) => error!("Connection accept failure: {}", e),
            }
        }
        Ok(())
    }

    pub fn process_xvc_stream<S: Read + Write>(&self, mut stream: S, hardware: &mut dyn JtagController) -> std::io::Result<()> {
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
                let configured_ns = hardware.set_tck_period(requested_ns);
                let mut reply = [0u8; 4];

                (&mut reply[..]).write_u32::<LittleEndian>(configured_ns)?;
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
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct MockJtagBackend {
        received_bits: u32,
        configured_period: u32,
    }

    impl JtagController for MockJtagBackend {
        fn set_tck_period(&mut self, period_ns: u32) -> u32 {
            self.configured_period = period_ns;
            period_ns * 2 // Return altered clock to simulate hardware limits
        }
        fn shift(&mut self, bits: u32, _tms: &[u8], _tdi: &[u8], tdo: &mut [u8]) {
            self.received_bits = bits;
            for byte in tdo.iter_mut() {
                *byte = 0x55; // Pattern fallback representation
            }
        }
    }

    #[test]
    fn test_getinfo_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").unwrap() };
        let mut mock_stream = Cursor::new(b"getinfo:".to_vec());
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw);
        assert_eq!(&mock_stream.into_inner()[8..], XVC_INFO_STRING);
    }

    #[test]
    fn test_settck_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").unwrap() };
        let mut payload = b"settck:\0".to_vec();
        payload.write_u32::<LittleEndian>(500).unwrap();

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw);
        assert_eq!(mock_hw.configured_period, 500);

        let out = mock_stream.into_inner();
        // The total request consumed 12 bytes (8 bytes padded header + 4 bytes value). Output is appended starting at index 12.
        let returned_period = (&out[12..16]).read_u32::<LittleEndian>().unwrap();
        assert_eq!(returned_period, 1000);
    }

    #[test]
    fn test_shift_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").unwrap() };
        let mut payload = b"shift:\x08\x00".to_vec(); // 8 bits command preamble (b0=8, b1=0)
        payload.write_u16::<LittleEndian>(0).unwrap(); // b2 = 0 (Total = 8 bits -> 1 byte payload)
        payload.push(0xFF); // TMS vector byte
        payload.push(0xAA); // TDI vector byte

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw);
        assert_eq!(mock_hw.received_bits, 8);

        let out = mock_stream.into_inner();
        assert_eq!(out[12], 0x55); // Confirm TDO modification tracking
    }
}
