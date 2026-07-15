use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

pub const XVC_INFO_STRING: &[u8] = b"xvcServer:v1.0\n";

pub trait JtagController {
    fn set_tck_period(&mut self, period_ns: u32) -> u32;
    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]);
}

pub fn process_xvc_stream<T: JtagController, S: Read + Write>(
    mut stream: S, 
    hardware: &mut T
) -> std::io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct MockJtagBackend { received_bits: u32 }
    impl JtagController for MockJtagBackend {
        fn set_tck_period(&mut self, period_ns: u32) -> u32 { period_ns }
        fn shift(&mut self, bits: u32, _tms: &[u8], _tdi: &[u8], tdo: &mut [u8]) {
            self.received_bits = bits;
            for byte in tdo.iter_mut() { *byte = 0xAA; }
        }
    }

    #[test]
    fn test_getinfo_command() {
        let mut mock_socket = Cursor::new(b"getinfo:".to_vec());
        let mut mock_hardware = MockJtagBackend { received_bits: 0 };
        let _ = process_xvc_stream(&mut mock_socket, &mut mock_hardware);
        assert_eq!(&mock_socket.into_inner()[8..], XVC_INFO_STRING);
    }
}
