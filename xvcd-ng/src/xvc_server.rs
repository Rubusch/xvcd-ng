use byteorder::{LittleEndian, ByteOrder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use log::{info, error};

pub const XVC_INFO_STRING: &[u8] = b"xvcServer:v1.0\n";

// Note:
// We explicitly allow 'async_fn_in_trait' and maintain 'async fn' signatures
// here because our underlying hardware engines (nusb and ftdi-nusb) rely on
// asynchronous I/O futures. Keeping these traits async ensures proper
// "tokio-cooperation" — when the server is waiting for USB turnaround
// microframes, it yields control back to the Tokio executor, preventing
// thread-stalls and keeping the network socket responsive.
#[allow(async_fn_in_trait)]
pub trait JtagController: Send + Sync {
    async fn set_tck_period(&mut self, period_ns: u32) -> u32;
    async fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) -> Result<(), String>;
}

pub struct XvcServer {
    listener: TcpListener,
}

impl XvcServer {
    pub async fn new(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        Ok(XvcServer { listener })
    }

    pub async fn run<T: JtagController + 'static>(&self, hardware: &mut T) -> std::io::Result<()> {
        info!("xvcd-ng async server listening for connections...");

        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Client connected: {:?}", addr);
                    let _ = stream.set_nodelay(true);
                    if let Err(e) = self.process_xvc_stream(stream, hardware).await {
                        error!("Session network error: {}", e);
                    }
                    info!("Client disconnected from {:?}", addr);
                }
                Err(e) => error!("Connection accept failure: {}", e),
            }
        }
    }

    async fn process_xvc_stream<S: AsyncReadExt + AsyncWriteExt + Unpin, T: JtagController>(&self, mut stream: S, hardware: &mut T) -> std::io::Result<()> {
        let mut buffer = [0u8; 8];

        loop {
            if stream.read_exact(&mut buffer).await.is_err() {
                break;
            }

            if &buffer[0..8] == b"getinfo:" {
                stream.write_all(XVC_INFO_STRING).await?;
                stream.flush().await?;

            } else if &buffer[0..7] == b"settck:" {
                let mut remaining_period = [0u8; 3];
                stream.read_exact(&mut remaining_period).await?;

                let mut period_buf = [0u8; 4];
                period_buf[0] = buffer[7];
                period_buf[1..4].copy_from_slice(&remaining_period);

                let requested_ns = LittleEndian::read_u32(&period_buf);
                let configured_ns = hardware.set_tck_period(requested_ns).await;
                let mut reply = [0u8; 4];

                LittleEndian::write_u32(&mut reply, configured_ns);
                stream.write_all(&reply).await?;
                stream.flush().await?;

            } else if &buffer[0..6] == b"shift:" {
                let b0 = buffer[6] as u32;
                let b1 = buffer[7] as u32;
                let b2 = stream.read_u16_le().await? as u32;
                let num_bits = b0 | (b1 << 8) | (b2 << 16);
                let byte_len = ((num_bits + 7) / 8) as usize;

                let mut tms_bytes = vec![0u8; byte_len];
                let mut tdi_bytes = vec![0u8; byte_len];
                let mut tdo_bytes = vec![0u8; byte_len];

                stream.read_exact(&mut tms_bytes).await?;
                stream.read_exact(&mut tdi_bytes).await?;

                if let Err(e) = hardware.shift(num_bits, &tms_bytes, &tdi_bytes, &mut tdo_bytes).await {
                    error!("JTAG Hardware operation error: {}", e);
                    break;
                }

                stream.write_all(&tdo_bytes).await?;
                stream.flush().await?;

            } else {
                error!("Protocol Violation: Unknown stream prefix {:?}", buffer);
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

    struct MockJtagBackend { received_bits: u32 }

    impl JtagController for MockJtagBackend {
        async fn set_tck_period(&mut self, period_ns: u32) -> u32 { period_ns * 2 }
        async fn shift(&mut self, bits: u32, _tms: &[u8], _tdi: &[u8], tdo: &mut [u8]) -> Result<(), String> {
            self.received_bits = bits;
            for byte in tdo.iter_mut() { *byte = 0x55; }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_getinfo_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").await.unwrap() };
        let mut mock_stream = Cursor::new(b"getinfo:".to_vec());
        let mut mock_hw = MockJtagBackend { received_bits: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        assert_eq!(&mock_stream.into_inner()[8..], XVC_INFO_STRING);
    }

    #[tokio::test]
    async fn test_settck_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").await.unwrap() };
        let mut payload = b"settck:\0".to_vec();
        payload.write_u32_le(500).await.unwrap();

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        let out = mock_stream.into_inner();
        let returned_period = LittleEndian::read_u32(&out[12..16]);
        assert_eq!(returned_period, 1000);
    }

    #[tokio::test]
    async fn test_shift_command() {
        let server = XvcServer { listener: TcpListener::bind("127.0.0.1:0").await.unwrap() };
        let mut payload = b"shift:\x08\x00".to_vec(); // 8 bits command preamble (b0=8, b1=0)
        payload.write_u16_le(0).await.unwrap(); // b2 = 0 (Total = 8 bits -> 1 byte payload)
        payload.push(0xFF); // TMS vector byte (1 byte)
        payload.push(0xAA); // TDI vector byte (1 byte)
        // Total input wire stream length = 12 bytes

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0 };

        let _ = server.process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        assert_eq!(mock_hw.received_bits, 8);

        let out = mock_stream.into_inner();
        // Since input consumed 12 bytes, the 1-byte TDO hardware result is at index 12
        assert_eq!(out[12], 0x55);
    }
}
