use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use log::{info, error};

pub const XVC_INFO_STRING: &[u8] = b"xvcServer:v1.0\n";

pub trait JtagController: Send + Sync {
    fn set_tck_period(&mut self, period_ns: u32) -> u32;
    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]);
}

pub struct XvcServer {
    listener: TcpListener,
}

impl XvcServer {
    pub async fn new(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        Ok(XvcServer { listener })
    }

    pub async fn run(&self, hardware: &'static mut (dyn JtagController + 'static)) -> std::io::Result<()> {
        info!("xvcd-ng async server listening for connections...");
        let hw_arc = std::sync::Arc::new(tokio::sync::Mutex::new(hardware));

        loop {
            match self.listener.accept().await {
                Ok((socket, addr)) => {
                    info!("Client connected: {:?}", addr);
                    let _ = socket.set_nodelay(true);

                    let local_hw = hw_arc.clone();
                    tokio::spawn(async move {
                        let mut hw_guard = local_hw.lock().await;
                        if let Err(e) = Self::process_xvc_stream(socket, *hw_guard).await {
                            error!("Session network error: {}", e);
                        }
                        info!("Client disconnected from {:?}", addr);
                    });
                }
                Err(e) => error!("Connection accept failure: {}", e),
            }
        }
    }

    pub async fn process_xvc_stream<S>(mut stream: S, hardware: &mut dyn JtagController) -> std::io::Result<()>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        let mut prefix_buf = [0u8; 6];

        loop {
            if stream.read_exact(&mut prefix_buf).await.is_err() {
                break;
            }

            if &prefix_buf[0..6] == b"getinf" {
                let mut suffix = [0u8; 2];
                stream.read_exact(&mut suffix).await?;

                stream.write_all(XVC_INFO_STRING).await?;
                stream.flush().await?;

            } else if &prefix_buf[0..6] == b"settck" {
                let mut suffix = [0u8; 2];
                stream.read_exact(&mut suffix).await?;

                let requested_ns = stream.read_u32_le().await?;
                let configured_ns = hardware.set_tck_period(requested_ns);

                stream.write_u32_le(configured_ns).await?;
                stream.flush().await?;

            } else if &prefix_buf[0..6] == b"shift:" {
                let num_bits = stream.read_u32_le().await?;
                let byte_len = ((num_bits + 7) / 8) as usize;

                let mut tms_bytes = vec![0u8; byte_len];
                let mut tdi_bytes = vec![0u8; byte_len];
                let mut tdo_bytes = vec![0u8; byte_len];

                stream.read_exact(&mut tms_bytes).await?;
                stream.read_exact(&mut tdi_bytes).await?;

                hardware.shift(num_bits, &tms_bytes, &tdi_bytes, &mut tdo_bytes);

                stream.write_all(&tdo_bytes).await?;
                stream.flush().await?;
            } else {
                error!("Protocol Violation: Unknown stream prefix {:?}", prefix_buf);
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
    unsafe impl Send for MockJtagBackend {}
    unsafe impl Sync for MockJtagBackend {}

    impl JtagController for MockJtagBackend {
        fn set_tck_period(&mut self, period_ns: u32) -> u32 {
            self.configured_period = period_ns;
            period_ns * 2
        }
        fn shift(&mut self, bits: u32, _tms: &[u8], _tdi: &[u8], tdo: &mut [u8]) {
            self.received_bits = bits;
            for byte in tdo.iter_mut() {
                *byte = 0x55;
            }
        }
    }

    #[tokio::test]
    async fn test_getinfo_command() {
        let mut mock_stream = Cursor::new(b"getinfo:".to_vec());
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = XvcServer::process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        assert_eq!(&mock_stream.into_inner()[8..], XVC_INFO_STRING);
    }

    #[tokio::test]
    async fn test_settck_command() {
        let mut payload = b"settck:".to_vec();
        payload.extend_from_slice(&500u32.to_le_bytes());

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = XvcServer::process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        assert_eq!(mock_hw.configured_period, 500);

        let out = mock_stream.into_inner();
        let mut reply_slice = &out[11..15];
        let returned_period = reply_slice.read_u32_le().await.unwrap();
        assert_eq!(returned_period, 1000);
    }

    #[tokio::test]
    async fn test_shift_command() {
        let mut payload = b"shift:".to_vec();
        payload.extend_from_slice(&8u32.to_le_bytes()); // 8 bits bitcount
        payload.push(0xFF); // TMS vector
        payload.push(0xAA); // TDI vector

        let mut mock_stream = Cursor::new(payload);
        let mut mock_hw = MockJtagBackend { received_bits: 0, configured_period: 0 };

        let _ = XvcServer::process_xvc_stream(&mut mock_stream, &mut mock_hw).await;
        assert_eq!(mock_hw.received_bits, 8);

        let out = mock_stream.into_inner();
        assert_eq!(out[10], 0x55); // Confirms correct TDO translation indexing
    }
}
