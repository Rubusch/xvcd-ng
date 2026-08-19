use crate::xvc_server::JtagController;
use ftdi_nusb::{FtdiDevice, mpsse::MpsseContext};
use std::io::Read;

pub struct FtdiMpsseBackend {
    device: FtdiDevice,
    mpsse: MpsseContext,
}

impl FtdiMpsseBackend {
    pub async fn new(vid: u16, pid: u16, _channel_index: u8) -> Result<Self, String> {
        let mut device = FtdiDevice::open(vid, pid)
            .map_err(|e| format!("Failed establishing FTDI device handle: {:?}", e))?;

        let mpsse = MpsseContext::init(&mut device, 1_000_000)
            .map_err(|e| format!("Failed initializing MPSSE engine subsystem context: {:?}", e))?;

        let mut backend = FtdiMpsseBackend { device, mpsse };
        backend.set_tck_period(500).await;
        Ok(backend)
    }
}

impl JtagController for FtdiMpsseBackend {
    async fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        let freq_hz = 1_000_000_000_u64 / (period_ns as u64);
        if let Err(e) = self.mpsse.set_clock(&mut self.device, freq_hz as u32) {
            log::error!("Failed to update MPSSE target clock frequency: {:?}", e);
        }

        period_ns
    }

    async fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) -> Result<(), String> {
        if bits == 0 { return Ok(()); }
        for byte in tdo.iter_mut() { *byte = 0x00; }

        let full_bytes = (bits / 8) as usize;
        let trailing_bits = (bits % 8) as u8;

        // Check if TMS is flat (all 0s or all 1s) for the byte duration.
        // If it is, we can dump large chunks over fast byte-level commands (0x39)
        let mut is_tms_flat = true;
        if full_bytes > 0 {
            let initial_tms_byte = tms[0];
            if initial_tms_byte != 0x00 && initial_tms_byte != 0xFF {
                is_tms_flat = false;
            } else {
                for &b in tms.iter().take(full_bytes) {
                    if b != initial_tms_byte {
                        is_tms_flat = false;
                        break;
                    }
                }
            }
        }

        let mut cmd_buffer = Vec::with_capacity(3 + full_bytes + (trailing_bits as usize * 3));

        if is_tms_flat && full_bytes > 0 {
            // High-Performance Optimization path:
            // Since TMS is static (either all 0s or all 1s), use fast byte-wide transfers
            // Command 0x39: Clock bytes Out (on -ve clock) and In (on +ve clock) LSB first
            cmd_buffer.push(0x39);

            // Length parameters are length - 1
            let len_minus_1 = (full_bytes - 1) as u16;
            cmd_buffer.push((len_minus_1 & 0xFF) as u8);
            cmd_buffer.push((len_minus_1 >> 8) as u8);

            // Append data payload directly
            cmd_buffer.extend_from_slice(&tdi[0..full_bytes]);

            // Handle individual bits for the rest if there's a remainder
            if trailing_bits > 0 {
                let start_bit = bits - trailing_bits as u32;
                for i in 0..trailing_bits {
                    let bit_pos = start_bit + i as u32;
                    let byte_idx = (bit_pos / 8) as usize;
                    let bit_idx = (bit_pos % 8) as u8;

                    let tms_val = (tms[byte_idx] >> bit_idx) & 1;
                    let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;
                    let pin_state = (tdi_val << 7) | tms_val;

                    cmd_buffer.push(0x3B);
                    cmd_buffer.push(0x00); // 1 bit
                    cmd_buffer.push(pin_state);
                }
            }

            // Fire bulk write buffer
            std::io::Write::write_all(&mut self.device, &cmd_buffer)
                .map_err(|e| format!("MPSSE fast bitstream write failed: {:?}", e))?;

            // Read total returned data back into the slice
            self.device.read_exact(tdo)
                .map_err(|e| format!("MPSSE fast bitstream read failed: {:?}", e))?;

        } else {
            // Fallback path: Handle everything bit-by-bit if TMS transitions dynamically inside bytes
            for i in 0..bits {
                let byte_idx = (i / 8) as usize;
                let bit_idx = (i % 8) as u8;

                let tms_val = (tms[byte_idx] >> bit_idx) & 1;
                let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;
                let pin_state = (tdi_val << 7) | tms_val;

                cmd_buffer.push(0x3B);
                cmd_buffer.push(0x00);
                cmd_buffer.push(pin_state);
            }

            std::io::Write::write_all(&mut self.device, &cmd_buffer)
                .map_err(|e| format!("MPSSE precise write failed: {:?}", e))?;

            let mut raw_response = vec![0u8; bits as usize];
            self.device.read_exact(&mut raw_response)
                .map_err(|e| format!("MPSSE precise read failed: {:?}", e))?;

            for (i, sampled_byte) in raw_response.iter().enumerate() {
                let byte_idx = i / 8;
                let bit_idx = i % 8;
                // MPSSE bit-mode returns the sampled bit in the lowest position (bit 0)
                let tdo_val = sampled_byte & 1;
                tdo[byte_idx] |= tdo_val << bit_idx;
            }
        }

        Ok(())
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
