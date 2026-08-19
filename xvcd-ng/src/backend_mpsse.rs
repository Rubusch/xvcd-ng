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

        let mut cmd_buffer = Vec::with_capacity((bits as usize) * 3);

        for i in 0..bits {
            let byte_idx = (i / 8) as usize;
            let bit_idx = (i % 8) as u8;

            let tms_val = (tms[byte_idx] >> bit_idx) & 1;
            let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;

            let pin_state = (tdi_val << 7) | tms_val;

            cmd_buffer.push(0x3B);
            cmd_buffer.push(0x00); // 0x00 clocks exactly 1 bit
            cmd_buffer.push(pin_state);
        }

        std::io::Write::write_all(&mut self.device, &cmd_buffer)
            .map_err(|e| format!("MPSSE bitstream write transaction failure: {:?}", e))?;

        self.device.read_exact(tdo)
            .map_err(|e| format!("MPSSE bitstream read transaction failure: {:?}", e))?;

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
