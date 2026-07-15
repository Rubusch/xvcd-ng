use crate::xvc_server::JtagController;
use rusb::{Context, DeviceHandle, UsbContext};
use std::time::Duration;

const TCK_PIN: u8  = 0x01; // Output (Bit 0)
const TDI_PIN: u8  = 0x02; // Output (Bit 1)
const TDO_PIN: u8  = 0x04; // Input  (Bit 2)
const TMS_PIN: u8  = 0x08; // Output (Bit 3)

const REQ_SET_BITMODE: u8 = 0x0B;
const REQ_READ_PINS: u8   = 0x0C;

pub struct FtdiBitbangBackend {
    handle: DeviceHandle<Context>,
    interface: u8,
}

impl FtdiBitbangBackend {
    pub fn new(vid: u16, pid: u16, channel_index: u8) -> Result<Self, String> {
        let context = Context::new().map_err(|e| format!("USB Context init fail: {}", e))?;
        let handle = context.open_device_with_vid_pid(vid, pid)
            .ok_or_else(|| format!("Could not find device with VID: {:04x} PID: {:04x}", vid, pid))?;

        let interface = channel_index;
        let _ = handle.detach_kernel_driver(interface);
        handle.claim_interface(interface)
            .map_err(|e| format!("Interface claim failed: {}", e))?;
        let index_routing_value = (interface as u16) + 1;

        // Configure Bitbang execution layer: Mask 0x0B, Mode 0x01 (Async Bitbang)
        let value = (0x01u16 << 8) | (TCK_PIN | TDI_PIN | TMS_PIN) as u16;
        handle.write_control(0x40, REQ_SET_BITMODE, value, index_routing_value, &[], Duration::from_millis(100))
            .map_err(|e| format!("Failed to set Bitbang Mode: {}", e))?;
        
        Ok(FtdiBitbangBackend { handle, interface })
    }
}

impl JtagController for FtdiBitbangBackend {
    fn set_tck_period(&mut self, period_ns: u32) -> u32 { period_ns }

    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) {
        for byte in tdo.iter_mut() { *byte = 0x00; }

        for i in 0..bits {
            let byte_idx = (i / 8) as usize;
            let bit_idx = (i % 8) as u8;

            let tms_val = (tms[byte_idx] >> bit_idx) & 1;
            let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;

            let mut pin_state = 0x00;
            if tms_val > 0 { pin_state |= TMS_PIN; }
            if tdi_val > 0 { pin_state |= TDI_PIN; }

            let mut buf = [pin_state];
            let _ = self.handle.write_bulk(0x02, &buf, Duration::from_millis(10));

            pin_state |= TCK_PIN;
            buf = [pin_state];
            let _ = self.handle.write_bulk(0x02, &buf, Duration::from_millis(10));

            let mut read_buf = [0u8; 1];
            let _ = self.handle.read_control(0xC0, REQ_READ_PINS, 0, (self.interface + 1) as u16, &mut read_buf, Duration::from_millis(10));

            let tdo_val = if (read_buf[0] & TDO_PIN) > 0 { 1u8 } else { 0u8 };
            tdo[byte_idx] |= tdo_val << bit_idx;
        }
    }
}
