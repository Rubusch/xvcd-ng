use crate::xvc_server::JtagController;
use rusb::{Context, DeviceHandle, UsbContext};
use std::time::Duration;

pub struct FtdiMpsseBackend {
    handle: DeviceHandle<Context>,
}

impl FtdiMpsseBackend {
    pub fn new(vid: u16, pid: u16) -> Result<Self, String> {
        let context = Context::new().map_err(|e| format!("USB Context init fail: {}", e))?;
        let handle = context.open_device_with_vid_pid(vid, pid)
            .ok_or_else(|| format!("Could not find device with VID: {:04x} PID: {:04x}", vid, pid))?;

        let interface = 0u8;
        let _ = handle.detach_kernel_driver(interface);
        handle.claim_interface(interface).map_err(|e| format!("Interface claim failed: {}", e))?;

        let value = (0x02u16 << 8) | 0x0B_u16;
        handle.write_control(0x40, 0x0B, value, (interface + 1) as u16, &[], Duration::from_millis(100))
            .map_err(|e| format!("Failed to set MPSSE mode: {}", e))?;

        let mut backend = FtdiMpsseBackend { handle };
        backend.set_tck_period(500);
        Ok(backend)
    }
}

impl JtagController for FtdiMpsseBackend {
    fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        let freq_hz = 1_000_000_000_u64 / (period_ns as u64);
        let divisor = if freq_hz >= 30_000_000 { 0u16 } else {
            let div_val = (30_000_000_u64 / freq_hz) - 1;
            if div_val > 0xFFFF { 0xFFFF } else { div_val as u16 }
        };

        let cmd = [0x86u8, (divisor & 0xFF) as u8, ((divisor >> 8) & 0xFF) as u8];
        let _ = self.handle.write_bulk(0x02, &cmd, Duration::from_millis(10));

        let actual_freq = 30_000_000 / (1 + divisor as u32);
        1_000_000_000 / actual_freq
    }

    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) {
        for byte in tdo.iter_mut() { *byte = 0x00; }
        let total_bytes = (bits / 8) as usize;
        let residual_bits = (bits % 8) as u8;

        if total_bytes > 0 {
            let length_arg = (total_bytes - 1) as u16;
            let header = [0x39u8, (length_arg & 0xFF) as u8, ((length_arg >> 8) & 0xFF) as u8];
            let _ = self.handle.write_bulk(0x02, &header, Duration::from_millis(10));

            for i in 0..total_bytes {
                let tms_state = [0x80u8, 0x00, 0x0B];
                let _ = self.handle.write_bulk(0x02, &tms_state, Duration::from_millis(10));
                let _ = self.handle.write_bulk(0x02, &[tdi[i]], Duration::from_millis(10));

                let mut resp = [0u8; 1];
                let _ = self.handle.read_bulk(0x81, &mut resp, Duration::from_millis(10));
                tdo[i] = resp[0];
            }
        }

        if residual_bits > 0 {
            let header = [0x3Bu8, residual_bits - 1];
            let _ = self.handle.write_bulk(0x02, &header, Duration::from_millis(10));

            let byte_pos = total_bytes;
            for b in 0..residual_bits {
                let tms_val = (tms[byte_pos] >> b) & 1;
                let tdi_val = (tdi[byte_pos] >> b) & 1;

                let mut pin_state = 0x00;
                if tms_val > 0 { pin_state |= 0x08; }
                if tdi_val > 0 { pin_state |= 0x02; }

                let tms_state = [0x80u8, pin_state, 0x0B];
                let _ = self.handle.write_bulk(0x02, &tms_state, Duration::from_millis(10));

                let mut resp = [0u8; 1];
                let _ = self.handle.read_bulk(0x81, &mut resp, Duration::from_millis(10));
                let tdo_val = if (resp[0] & 0x04) > 0 { 1u8 } else { 0u8 };

                tdo[byte_pos] |= tdo_val << b;
            }
        }
        let _ = self.handle.write_bulk(0x02, &[0x87u8], Duration::from_millis(10));
    }
}
