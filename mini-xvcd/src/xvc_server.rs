use byteorder::{LittleEndian, ByteOrder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use log::{info, error, debug, trace};


// Note:
// We explicitly allow 'async_fn_in_trait' and maintain 'async fn' signatures
// here because our underlying hardware engines (nusb and ftdi-nusb) rely on
// asynchronous I/O futures. Keeping these traits async ensures proper
// "tokio-cooperation" — when the server is waiting for USB turnaround
// microframes, it yields control back to the Tokio executor, preventing
// thread-stalls and keeping the network socket responsive.
pub const XVC_INFO_STRING: &[u8] = b"xvcServer_v1.0:2048\n";

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
        info!("mini-xvcd async server listening for connections...");

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
        let mut cmd_buf = Vec::with_capacity(4096);
        let mut read_raw_buf = [0u8; 2048];

        loop {
            let n = match stream.read(&mut read_raw_buf).await {
                Ok(0) => {
                    debug!("Client clean disconnect or end of stream reached.");
                    break;
                }
                Ok(bytes_read) => bytes_read,
                Err(e) => {
                    error!("Socket processing execution exception: {}", e);
                    return Err(e);
                }
            };

            cmd_buf.extend_from_slice(&read_raw_buf[0..n]);

            while !cmd_buf.is_empty() {
                if cmd_buf.starts_with(b"getinfo:") {
                    debug!("Received 'getinfo:' command token.");
                    stream.write_all(XVC_INFO_STRING).await?;
                    stream.flush().await?;
                    cmd_buf.drain(0..8);

                } else if cmd_buf.starts_with(b"settck:") {
                    if cmd_buf.len() < 7 + 4 {
                        break;
                    }

                    let requested_ns = LittleEndian::read_u32(&cmd_buf[7..11]);
                    debug!("Received 'settck:' command token. Requested period: {} ns", requested_ns);

                    let configured_ns = hardware.set_tck_period(requested_ns).await;
                    debug!("Hardware configured clock period: {} ns", configured_ns);

                    let mut reply = [0u8; 4];
                    LittleEndian::write_u32(&mut reply, configured_ns);
                    stream.write_all(&reply).await?;
                    stream.flush().await?;

                    cmd_buf.drain(0..(7 + 4));

                } else if cmd_buf.starts_with(b"shift:") {
                    if cmd_buf.len() < 6 + 4 {
                        break;
                    }

                    let num_bits = LittleEndian::read_u32(&cmd_buf[6..10]);
                    let byte_len = ((num_bits + 7) / 8) as usize;
                    let total_expected_packet_len = 6 + 4 + (byte_len * 2);

                    if cmd_buf.len() < total_expected_packet_len {
                        break;
                    }

                    debug!("Received 'shift:' command token. Shifting {} bits ({} bytes payload)", num_bits, byte_len);

                    let start_tms_idx = 6 + 4;
                    let start_tdi_idx = start_tms_idx + byte_len;
                    let end_packet_idx = start_tdi_idx + byte_len;

                    // FIX: Isolate data allocations completely into owned arrays before dispatching to the hardware thread pool
                    let tms_bytes = cmd_buf[start_tms_idx..start_tdi_idx].to_vec();
                    let tdi_bytes = cmd_buf[start_tdi_idx..end_packet_idx].to_vec();
                    let mut tdo_bytes = vec![0u8; byte_len];

                    trace!("Executing JTAG hardware shift sequence flight...");
                    if let Err(e) = hardware.shift(num_bits, &tms_bytes, &tdi_bytes, &mut tdo_bytes).await {
                        error!("JTAG hardware shift engine execution failure: {}", e);
                        return Ok(());
                    }

                    trace!("Flushing sampled TDO array back to network socket.");
                    stream.write_all(&tdo_bytes).await?;
                    stream.flush().await?;

                    cmd_buf.drain(0..total_expected_packet_len);

                } else {
                    if cmd_buf.len() > 16 {
                        error!("Protocol alignment fault. Unknown lookup stream lookahead signature: {:?}", String::from_utf8_lossy(&cmd_buf));
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "XVC framing out-of-sync alignment fault."));
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}
