use crate::xvc_server::JtagController;
use nusb::MaybeFuture;
use nusb::transfer::{ControlOut, ControlType, Recipient, Bulk, In, Out, Buffer};
use std::time::Duration;

pub struct FtdiMpsseBackend {
    _device_handle: nusb::Interface,
    out_endpoint: nusb::Endpoint<Bulk, Out>,
    in_endpoint: nusb::Endpoint<Bulk, In>,
}

impl FtdiMpsseBackend {
    pub fn new(vid: u16, pid: u16, channel_index: u8) -> Result<Self, String> {
        let mut devices = nusb::list_devices()
            .wait()
            .map_err(|e| format!("Failed listing USB layout: {}", e))?;

        let info = devices
            .find(|d| d.vendor_id() == vid && d.product_id() == pid)
            .ok_or_else(|| format!("Could not locate device with VID: {:04x} PID: {:04x}", vid, pid))?;

        let device = info.open().wait().map_err(|e| format!("Failed opening device handle: {}", e))?;

        // Claim the interface corresponding to the selected channel index
        let interface = device.claim_interface(channel_index)
            .wait()
            .map_err(|e| format!("Failed to claim interface {}: {}", channel_index, e))?;

        // Standard FTDI channel endpoints map deterministically:
        // Channel A -> Out: 0x02, In: 0x81; Channel B -> Out: 0x04, In: 0x83 etc.
        let out_addr = 0x02 + (channel_index * 2);
        let in_addr = 0x81 + (channel_index * 2);

        let out_endpoint = interface.endpoint::<Bulk, Out>(out_addr)
            .map_err(|e| format!("Failed to open OUT endpoint: {}", e))?;
        let in_endpoint = interface.endpoint::<Bulk, In>(in_addr)
            .map_err(|e| format!("Failed to open IN endpoint: {}", e))?;

        let index_routing_value = (channel_index as u16) + 1;

        // Reset the interface engine target over control pipe
        interface.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0x00,
            value: 0,
            index: index_routing_value,
            data: &[],
        }, Duration::from_millis(100))
        .wait()
        .map_err(|e| format!("Reset failed: {:?}", e))?;

        // Establish target MPSSE engine: Mode 0x02 (MPSSE)
        let mpsse_mode_value = (0x02u16 << 8) | 0x0B_u16; // 0x0B = Bitmode request
        interface.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0x0B,
            value: mpsse_mode_value,
            index: index_routing_value,
            data: &[],
        }, Duration::from_millis(100))
        .wait()
        .map_err(|e| format!("MPSSE activation fail: {:?}", e))?;

        let mut backend = FtdiMpsseBackend {
            _device_handle: interface,
            out_endpoint,
            in_endpoint,
        };

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

        // Command 0x86 sets the clock divisor
        let cmd = [0x86u8, (divisor & 0xFF) as u8, ((divisor >> 8) & 0xFF) as u8];
        let _ = self.out_endpoint.transfer_blocking(Buffer::from(cmd.to_vec()), Duration::from_millis(10));

        let actual_freq = 30_000_000 / (1 + divisor as u32);
        1_000_000_000 / actual_freq
    }

    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) {
        for byte in tdo.iter_mut() { *byte = 0x00; }
        if bits == 0 { return; }

        let mut cmd_buffer = Vec::with_capacity(bits as usize * 3);
        let mut expected_read_bytes = 0;

        generate_mpsse_payload(bits, tms, tdi, &mut cmd_buffer, &mut expected_read_bytes);

        if !cmd_buffer.is_empty() {
            let tx_buffer = Buffer::from(cmd_buffer);
            let tx_completion = self.out_endpoint.transfer_blocking(tx_buffer, Duration::from_millis(1000));

            if tx_completion.status.is_ok() {
                let rx_alloc = Buffer::new(expected_read_bytes);
                let rx_completion = self.in_endpoint.transfer_blocking(rx_alloc, Duration::from_millis(1000));

                if rx_completion.status.is_ok() {
                    parse_mpsse_response(bits, &rx_completion.buffer, tdo);
                }
            }
        }
    }
}

fn generate_mpsse_payload(bits: u32, tms: &[u8], tdi: &[u8], cmd_buffer: &mut Vec<u8>, expected_read_bytes: &mut usize) {
    for i in 0..bits {
        let byte_idx = (i / 8) as usize;
        let bit_idx = (i % 8) as u8;

        let tms_val = (tms[byte_idx] >> bit_idx) & 1;
        let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;

        // Command 0x3B pin layout: Bit 7 = TDI (shifted to MSB), Bit 0 = TMS
        let pin_state = (tdi_val << 7) | tms_val;

        cmd_buffer.push(0x3B);
        cmd_buffer.push(0x00);
        cmd_buffer.push(pin_state);
        *expected_read_bytes += 1;
    }
}

fn parse_mpsse_response(bits: u32, read_buffer: &[u8], tdo: &mut [u8]) {
    for i in 0..bits {
        let byte_idx = (i / 8) as usize;
        let bit_idx = (i % 8) as u8;

        // Command 0x3B captures input via Bit 0 (LSB) of the received byte
        let tdo_bit = read_buffer[i as usize] & 0x01;
        tdo[byte_idx] |= tdo_bit << bit_idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mpsse_payload_batch_generation() {
        let bits = 2;
        let tms = [0x01];
        let tdi = [0x02];

        let mut cmd_buffer = Vec::new();
        let mut expected_read_bytes = 0;

        generate_mpsse_payload(bits, &tms, &tdi, &mut cmd_buffer, &mut expected_read_bytes);

        assert_eq!(cmd_buffer.len(), 6);
        assert_eq!(expected_read_bytes, 2);

        assert_eq!(cmd_buffer[0], 0x3B);
        assert_eq!(cmd_buffer[2], 0x01);
        assert_eq!(cmd_buffer[3], 0x3B);
        assert_eq!(cmd_buffer[5], 0x80);
    }

    #[test]
    fn test_mpsse_residual_bit_parsing_alignment() {
        let bits = 2;
        let mut tdo = [0x00];
        let mock_read_hardware_buffer = [0x01, 0x00]; // Active bit on LSB (Bit 0)

        parse_mpsse_response(bits, &mock_read_hardware_buffer, &mut tdo);
        assert_eq!(tdo[0], 0x01);
    }
}
