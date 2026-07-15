// src/backend.rs

/// Abstract interface mapping the network protocol commands to hardware execution engines.
pub trait JtagController {
    /// Configures the TCK clock frequency based on the requested period in nanoseconds.
    fn set_tck_period(&mut self, period_ns: u32) -> u32;

    /// Shifts a sequence of bits through the JTAG chain.
    fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]);
}

/// Dummy placeholder driver used to debug network frame integrity.
pub struct DummyBackend;

impl JtagController for DummyBackend {
    fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        period_ns
    }

    fn shift(&mut self, _bits: u32, _tms: &[u8], _tdi: &[u8], tdo: &mut [u8]) {
        for byte in tdo.iter_mut() {
            *byte = 0x00; 
        }
    }
}
