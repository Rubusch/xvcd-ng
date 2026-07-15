use crate::xvc_server::JtagController;
use libftdi1_sys::{
    ftdi_context, ftdi_free, ftdi_new, ftdi_set_bitmode, ftdi_usb_close, 
    ftdi_usb_open, ftdi_write_data, ftdi_read_data
};

const TCK_PIN: u8  = 0x01; // Output (Bit 0)
const TDI_PIN: u8  = 0x02; // Output (Bit 1)
const TDO_PIN: u8  = 0x04; // Input  (Bit 2)
const TMS_PIN: u8  = 0x08; // Output (Bit 3)

pub struct FtdiBitbangBackend {
    ctx: *mut ftdi_context,
}

impl FtdiBitbangBackend {
    pub fn new(vid: u16, pid: u16) -> Result<Self, String> {
        unsafe {
            let ctx = ftdi_new();
            if ctx.is_null() {
                return Err("Failed to allocate FTDI context context".to_string());
            }

            let ret = ftdi_usb_open(ctx, vid as i32, pid as i32);
            if ret < 0 {
                ftdi_free(ctx);
                return Err(format!("Failed to open FTDI device: {}", ret));
            }

            // Configure Pins: TCK, TDI, TMS as output (1), TDO as input (0)
            // Mask: 0x01 | 0x02 | 0x08 = 0x0B. Mode: 0x01 (Asynchronous BitBang)
            let direction_mask = TCK_PIN | TDI_PIN | TMS_PIN;
            let ret = ftdi_set_bitmode(ctx, direction_mask, 0x01); // 0x01 = Async Bitbang
            if ret < 0 {
                ftdi_usb_close(ctx);
                ftdi_free(ctx);
                return Err(format!("Failed to set bitbang mode: {}", ret));
            }

            Ok(FtdiBitbangBackend { ctx })
        }
    }
}

impl Drop for FtdiBitbangBackend {
    fn drop(&mut self) {
        unsafe {
            if !self.ctx.is_null() {
                ftdi_usb_close(self.ctx);
                ftdi_free(self.ctx);
            }
        }
    }
}

impl JtagController for FtdiBitbangBackend {
    fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        // Bitbang clock speeds are strictly bounded by raw USB framing latency limits
        period_ns 
    }

    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) {
        unsafe {
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
                ftdi_write_data(self.ctx, buf.as_mut_ptr(), 1);

                pin_state |= TCK_PIN;
                buf = [pin_state];
                ftdi_write_data(self.ctx, buf.as_mut_ptr(), 1);

                let mut read_buf = [0u8];
                ftdi_read_data(self.ctx, read_buf.as_mut_ptr(), 1);
                let tdo_val = if (read_buf[0] & TDO_PIN) > 0 { 1u8 } else { 0u8 };

                tdo[byte_idx] |= tdo_val << bit_idx;
            }
        }
    }
}
