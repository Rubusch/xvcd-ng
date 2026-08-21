use crate::xvc_server::JtagController;
use nusb::transfer::{ControlOut, ControlType, Recipient, Bulk, In, Out, Direction};
use std::time::Duration;
use log::{trace, warn};

const TCK_PIN: u8  = 0x01; // Output (Bit 0)
const TDI_PIN: u8  = 0x02; // Output (Bit 1)
const TDO_PIN: u8  = 0x04; // Input  (Bit 2)
const TMS_PIN: u8  = 0x08; // Output (Bit 3)

const MAX_BIT_CHUNK_SIZE: u32 = 512;

pub struct FtdiBitbangBackend {
    _interface_handle: nusb::Interface,
    out_endpoint: nusb::Endpoint<Bulk, Out>,
    in_endpoint: nusb::Endpoint<Bulk, In>,
}

impl FtdiBitbangBackend {
    pub async fn new(vid: u16, pid: u16, channel_index: u8) -> Result<Self, String> {
        let devices = nusb::list_devices()
            .await
            .map_err(|e| format!("Failed listing USB layout: {}", e))?;

        let info = devices
            .into_iter()
            .find(|d| d.vendor_id() == vid && d.product_id() == pid)
            .ok_or_else(|| format!("Could not locate device with VID: {:04x} PID: {:04x}", vid, pid))?;

        let device = info.open().await.map_err(|e| format!("Failed opening device handle: {}", e))?;
        let interface_handle = device.detach_and_claim_interface(channel_index)
            .await
            .map_err(|e| format!("Failed to detach kernel and claim interface {}: {}", channel_index, e))?;

        let di = interface_handle.descriptors()
            .find(|d| d.interface_number() == channel_index)
            .ok_or_else(|| format!("Could not find descriptor metadata for interface {}", channel_index))?;

        let mut out_addr = None;
        let mut in_addr = None;

        for ep in di.endpoints() {
            match ep.direction() {
                Direction::In => in_addr = Some(ep.address()),
                Direction::Out => out_addr = Some(ep.address()),
            }
        }

        let out_addr = out_addr.ok_or("Could not resolve OUT endpoint address")?;
        let in_addr = in_addr.ok_or("Could not resolve IN endpoint address")?;

        let out_endpoint = interface_handle.endpoint(out_addr)
            .map_err(|e| format!("Failed to open OUT endpoint: {}", e))?;
        let in_endpoint = interface_handle.endpoint(in_addr)
            .map_err(|e| format!("Failed to open IN endpoint: {}", e))?;

        let index_routing_value = (channel_index as u16) + 1;

        interface_handle.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0x00, // FTDI Reset command
            value: 0,
            index: index_routing_value,
            data: &[],
        }, Duration::from_millis(50))
        .await
        .map_err(|e| format!("Reset failed: {:?}", e))?;

        interface_handle.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0x09, // SET_LATENCY_TIMER
            value: 2,      // 2ms latency target
            index: index_routing_value,
            data: &[],
        }, Duration::from_millis(50))
        .await
        .map_err(|e| format!("Failed to set latency timer: {:?}", e))?;

        // Mode 0x01 maps to Asynchronous Bitbang over standard FTDI chips
        // High byte: Mode Selection (0x01) | Low byte: Pin Direction Configuration (Mask)
        let direction_mask = TCK_PIN | TDI_PIN | TMS_PIN;
        let bitmode_value = (0x01u16 << 8) | (direction_mask as u16);

        interface_handle.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0x0B, // SET_BITMODE
            value: bitmode_value,
            index: index_routing_value,
            data: &[],
        }, Duration::from_millis(50))
        .await
        .map_err(|e| format!("Failed to set Bitbang Mode: {:?}", e))?;

        Ok(FtdiBitbangBackend {
            _interface_handle: interface_handle,
            out_endpoint,
            in_endpoint
        })
    }
}

impl JtagController for FtdiBitbangBackend {
    async fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        period_ns
    }

    async fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) -> Result<(), String> {
        for byte in tdo.iter_mut() { *byte = 0x00; }
        if bits == 0 { return Ok(()); }

        let mut bits_processed = 0u32;

        while bits_processed < bits {
            let chunk_bits = std::cmp::min(bits - bits_processed, MAX_BIT_CHUNK_SIZE);
            let num_steps = chunk_bits as usize * 2;
            let mut cmd_buffer = Vec::with_capacity(num_steps);

            for i in 0..chunk_bits {
                let absolute_bit_idx = bits_processed + i;
                let byte_idx = (absolute_bit_idx / 8) as usize;
                let bit_idx = (absolute_bit_idx % 8) as u8;

                let tms_val = (tms[byte_idx] >> bit_idx) & 1;
                let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;

                let mut pin_base = 0x00;
                if tms_val > 0 { pin_base |= TMS_PIN; }
                if tdi_val > 0 { pin_base |= TDI_PIN; }

                cmd_buffer.push(pin_base);
                cmd_buffer.push(pin_base | TCK_PIN);
            }

            let expected_rx_len = num_steps + 2;

            let tx_fut = async {
                self.out_endpoint.submit(cmd_buffer.into());
                self.out_endpoint.next_complete().await
            };

            let rx_fut = async {
                self.in_endpoint.submit(nusb::transfer::Buffer::new(expected_rx_len));
                self.in_endpoint.next_complete().await
            };

            let (tx_res, rx_res) = tokio::join!(tx_fut, rx_fut);

            tx_res.status.map_err(|e| format!("USB TX Execution Error: {:?}", e))?;
            rx_res.status.map_err(|e| format!("USB RX Execution Error: {:?}", e))?;

            if rx_res.buffer.len() < expected_rx_len {
                warn!("Short read detected: expected {} bytes, got {}", expected_rx_len, rx_res.buffer.len());
                return Err("FTDI incomplete chunk capture error during shift sequence.".to_string());
            }

            let payload = &rx_res.buffer[2..];

            for i in 0..chunk_bits {
                let absolute_bit_idx = bits_processed + i;
                let byte_idx = (absolute_bit_idx / 8) as usize;
                let bit_idx = (absolute_bit_idx % 8) as u8;

                let step_offset = (i as usize * 2) + 1;
                let sampled_pins = payload[step_offset];

                let tdo_val = if (sampled_pins & TDO_PIN) > 0 { 1u8 } else { 0u8 };
                tdo[byte_idx] |= tdo_val << bit_idx;
            }

            bits_processed += chunk_bits;
        }

        trace!("Bitbang hardware shift phase complete for {} bits.", bits);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_bitbang_vector_generation_logic() {
        let bits = 2;
        let tms = [0x01];
        let tdi = [0x02];
        let mut cmd_buffer = Vec::new();

        const TCK_PIN: u8 = 0x01;
        const TDI_PIN: u8 = 0x02;
        const TMS_PIN: u8 = 0x08;

        for i in 0..bits {
            let byte_idx = (i / 8) as usize;
            let bit_idx = (i % 8) as u8;
            let tms_val = (tms[byte_idx] >> bit_idx) & 1;
            let tdi_val = (tdi[byte_idx] >> bit_idx) & 1;

            let mut pin_base = 0x00;
            if tms_val > 0 { pin_base |= TMS_PIN; }
            if tdi_val > 0 { pin_base |= TDI_PIN; }
            cmd_buffer.push(pin_base);
            cmd_buffer.push(pin_base | TCK_PIN);
        }

        assert_eq!(cmd_buffer.len(), 4);
        assert_eq!(cmd_buffer[0], 0x08); // Low clock: TMS=1, TDI=0
        assert_eq!(cmd_buffer[1], 0x09); // High clock: TCK | TMS
        assert_eq!(cmd_buffer[2], 0x02); // Low clock: TMS=0, TDI=1
        assert_eq!(cmd_buffer[3], 0x03);
    }
}
