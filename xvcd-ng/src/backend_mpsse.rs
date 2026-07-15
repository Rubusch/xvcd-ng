use crate::xvc_server::JtagController;

pub struct FtdiMpsseBackend {
    // TODO
}

impl FtdiMpsseBackend {
    pub fn new(_vid: u16, _pid: u16) -> Result<Self, String> {
        // TODO placeholder for setting up bitmode 0x02 (MPSSE Hardware Acceleration)
        Ok(FtdiMpsseBackend {})
    }
}

impl JtagController for FtdiMpsseBackend {
    fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        // TODO mapping calculations for the internal MPSSE clock divisors
        period_ns
    }

    fn shift(&mut self, _bits: u32, _tms: &[u8], _tdi: &[u8], _tdo: &mut [u8]) {
        // TODO home of serializing arrays to 0x39 / 0x3B raw MPSSE byte vectors
    }
}
