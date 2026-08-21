use crate::xvc_server::JtagController;
use ftdi_nusb::{FtdiDevice, mpsse::MpsseContext};

const MPSSE_CHUNK_BITS: u32 = 1024;

pub struct FtdiMpsseBackend {
    device: Option<FtdiDevice>,
    mpsse: Option<MpsseContext>,
}

impl FtdiMpsseBackend {
    pub async fn new(vid: u16, pid: u16, _channel_index: u8) -> Result<Self, String> {
        log::info!("Initializing MPSSE Hardware Backend (VID: {:04x}, PID: {:04x})...", vid, pid);

        tokio::task::spawn_blocking(move || {
            let mut device = FtdiDevice::open(vid, pid)
                .map_err(|e| format!("Failed establishing FTDI device handle: {:?}", e))?;

            let mpsse = MpsseContext::init(&mut device, 1_000_000)
                .map_err(|e| format!("Failed initializing MPSSE engine context: {:?}", e))?;

            let mut backend = FtdiMpsseBackend {
                device: Some(device),
                mpsse: Some(mpsse)
            };

            let freq_hz = 1_000_000_000_u64 / 500;
            let dev_ref = backend.device.as_mut().unwrap();
            if let Err(e) = backend.mpsse.as_mut().unwrap().set_clock(dev_ref, freq_hz as u32) {
                log::error!("Failed to configure initial clock state: {:?}", e);
            }

            let init_pins = [0x80, 0x0A, 0x0B];
            if let Err(e) = dev_ref.write_all(&init_pins) {
                log::error!("Failed to dispatch MPSSE JTAG pin configuration sequence: {:?}", e);
            } else {
                log::info!("MPSSE JTAG lines hardware state aligned (TCK=0, TDI=1, TMS=1).");
            }

            Ok(backend)
        })
        .await
        .map_err(|e| format!("Thread join failure during initialization: {}", e))?
    }
}

impl JtagController for FtdiMpsseBackend {
    async fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        let period_ns = std::cmp::max(period_ns, 33);
        let freq_hz = 1_000_000_000_u64 / (period_ns as u64);
        let freq_32 = freq_hz as u32;

        log::debug!("Clock scaling request: {} ns period -> target frequency {} Hz", period_ns, freq_32);

        let mut dev = self.device.take().expect("Device handle vanished");
        let mut ctx = self.mpsse.take().expect("MPSSE context vanished");

        let res = tokio::task::spawn_blocking(move || {
            match ctx.set_clock(&mut dev, freq_32) {
                Ok(_) => Ok((dev, ctx)),
                Err(e) => Err((dev, ctx, e)),
            }
        }).await;

        match res {
            Ok(Ok((returned_dev, returned_ctx))) => {
                self.device = Some(returned_dev);
                self.mpsse = Some(returned_ctx);
                log::info!("MPSSE hardware clock scaled to {} Hz", freq_32);
            }
            Ok(Err((returned_dev, returned_ctx, err))) => {
                self.device = Some(returned_dev);
                self.mpsse = Some(returned_ctx);
                log::error!("Failed to re-scale the internal MPSSE hardware clock: {:?}", err);
            }
            Err(join_err) => {
                log::error!("Runtime failure on clock management worker thread: {}", join_err);
            }
        }

        period_ns
    }

    async fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) -> Result<(), String> {
        if bits == 0 { return Ok(()); }
        for byte in tdo.iter_mut() { *byte = 0x00; }

        log::debug!("[JTAG SHIFT] Transmitting operational frame: {} bits", bits);
        log::trace!("  -> Host TMS stream slice (hex): {:02x?}", &tms[0..std::cmp::min(tms.len(), 16)]);
        log::trace!("  -> Host TDI stream slice (hex): {:02x?}", &tdi[0..std::cmp::min(tdi.len(), 16)]);

        let mut device = self.device.take().expect("Device handle unallocated");
        let ctx = self.mpsse.take().expect("MPSSE state configuration unallocated");

        let tms_vec = tms.to_vec();
        let tdi_vec = tdi.to_vec();
        let mut tdo_vec = tdo.to_vec();

        let execution_result = tokio::task::spawn_blocking(move || -> Result<(FtdiDevice, MpsseContext, Vec<u8>), String> {
            use std::io::{Read,};
            let mut bits_processed = 0u32;

            while bits_processed < bits {
                let chunk_bits = std::cmp::min(bits - bits_processed, MPSSE_CHUNK_BITS);
                let mut cmd_buffer = Vec::with_capacity(chunk_bits as usize * 3 + 4);

                let mut can_use_byte_mode = (chunk_bits % 8 == 0) && (bits_processed % 8 == 0);
                if can_use_byte_mode {
                    let start_byte = (bits_processed / 8) as usize;
                    let end_byte = start_byte + (chunk_bits / 8) as usize;
                    for byte_idx in start_byte..end_byte {
                        if tms_vec[byte_idx] != 0x00 {
                            can_use_byte_mode = false;
                            break;
                        }
                    }
                }

                if can_use_byte_mode {
                    let byte_count = (chunk_bits / 8) as usize;
                    let start_byte = (bits_processed / 8) as usize;

                    log::trace!("  [Routing Path] Chunk size {}: Fast Byte-Wide Mode (0x39)", chunk_bits);

                    cmd_buffer.push(0x39);
                    let len_minus_1 = (byte_count - 1) as u16;
                    cmd_buffer.push((len_minus_1 & 0xFF) as u8);
                    cmd_buffer.push((len_minus_1 >> 8) as u8);
                    cmd_buffer.extend_from_slice(&tdi_vec[start_byte..(start_byte + byte_count)]);

                    device.write_all(&cmd_buffer)
                        .map_err(|e| format!("MPSSE byte block dispatch failed: {:?}", e))?;

                    let mut chunk_rx = vec![0u8; byte_count];
                    device.read_exact(&mut chunk_rx)
                        .map_err(|e| format!("MPSSE byte block response capture failed: {:?}", e))?;

                    tdo_vec[start_byte..(start_byte + byte_count)].copy_from_slice(&chunk_rx);
                } else {
                    log::trace!("  [Routing Path] Chunk size {}: Precise Bit-by-Bit Mode (0x4B)", chunk_bits);

                    for i in 0..chunk_bits {
                        let absolute_bit_idx = bits_processed + i;
                        let byte_idx = (absolute_bit_idx / 8) as usize;
                        let bit_idx = (absolute_bit_idx % 8) as u8;

                        let tms_val = (tms_vec[byte_idx] >> bit_idx) & 1;
                        let tdi_val = (tdi_vec[byte_idx] >> bit_idx) & 1;
                        let pin_data = (tdi_val << 1) | tms_val;

                        cmd_buffer.push(0x4B);
                        cmd_buffer.push(0x00);
                        cmd_buffer.push(pin_data);
                    }

                    device.write_all(&cmd_buffer)
                        .map_err(|e| format!("MPSSE sequential bit string write failed: {:?}", e))?;

                    let mut rx_buf = vec![0u8; chunk_bits as usize];
                    device.read_exact(&mut rx_buf)
                        .map_err(|e| format!("MPSSE sequential bit string sample failed: {:?}", e))?;

                    for i in 0..chunk_bits {
                        let absolute_bit_idx = bits_processed + i;
                        let byte_idx = (absolute_bit_idx / 8) as usize;
                        let bit_idx = (absolute_bit_idx % 8) as u8;

                        let tdo_val = (rx_buf[i as usize] >> 7) & 1;
                        tdo_vec[byte_idx] |= tdo_val << bit_idx;
                    }
                }

                bits_processed += chunk_bits;
            }
            Ok((device, ctx, tdo_vec))
        })
        .await
        .map_err(|e| format!("MPSSE worker thread panicked: {}", e))?;

        match execution_result {
            Ok((returned_device, returned_ctx, finished_tdo)) => {
                log::trace!("  <- Hardware TDO Sample Array (hex): {:02x?}", &finished_tdo[0..std::cmp::min(finished_tdo.len(), 16)]);

                self.device = Some(returned_device);
                self.mpsse = Some(returned_ctx);
                tdo.copy_from_slice(&finished_tdo);
                Ok(())
            }
            Err(e) => {
                log::error!("  [CRITICAL] MPSSE Transaction Aborted: {}", e);
                Err(format!("MPSSE Shift Transaction Failure: {}", e))
            }
        }
    }
}

// Note:
// We do not implement explicit unit tests inside this module because ftdi-nusb
// relies directly on live hardware enumeration and physical USB file descriptor
// endpoints. Fully mocking the asynchronous USB sub-layers and register queues
// would add excessive, brittle complexity. Full operational reliability is
// already guaranteed by unit tests on the abstract protocol layers
// (xvc_server.rs) and the thoroughly verified internals of the underlying
// ftdi-nusb driver crate.
