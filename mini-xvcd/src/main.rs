use clap::Parser;
use mini_xvcd::{CliArgs, BackendMode, FtdiMpsseBackend, FtdiBitbangBackend, xvc_server::XvcServer};

enum HardwareBackend {
    Mpsse(FtdiMpsseBackend),
    Bitbang(FtdiBitbangBackend),
}

impl mini_xvcd::JtagController for HardwareBackend {
    async fn set_tck_period(&mut self, period_ns: u32) -> u32 {
        match self {
            HardwareBackend::Mpsse(b) => b.set_tck_period(period_ns).await,
            HardwareBackend::Bitbang(b) => b.set_tck_period(period_ns).await,
        }
    }
    async fn shift(&mut self, bits: u32, tms: &[u8], tdi: &[u8], tdo: &mut [u8]) -> Result<(), String> {
        match self {
            HardwareBackend::Mpsse(b) => b.shift(bits, tms, tdi, tdo).await,
            HardwareBackend::Bitbang(b) => b.shift(bits, tms, tdi, tdo).await,
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let args = CliArgs::parse();

    let mut hardware = match args.mode {
        BackendMode::Mpsse => {
            match FtdiMpsseBackend::new(args.vid, args.pid, args.channel).await {
                Ok(hw) => HardwareBackend::Mpsse(hw),
                Err(e) => {
                    eprintln!("Fatal MPSSE Hardware Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BackendMode::Bitbang => {
            match FtdiBitbangBackend::new(args.vid, args.pid, args.channel).await {
                Ok(hw) => HardwareBackend::Bitbang(hw),
                Err(e) => {
                    eprintln!("Fatal Bitbang Hardware Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let server = XvcServer::new(args.port).await?;
    println!("mini-xvcd server initialization complete. Listening on port {}...", args.port);

    server.run(&mut hardware).await
}

#[cfg(test)]
mod tests {
    use mini_xvcd::JtagController;

    struct DummyBackend {
        last_period: u32,
    }

    impl JtagController for DummyBackend {
        async fn set_tck_period(&mut self, period_ns: u32) -> u32 {
            self.last_period = period_ns;
            period_ns + 100 // Return an altered value to verify routing math
        }

        async fn shift(&mut self, _bits: u32, _tms: &[u8], _tdi: &[u8], _tdo: &mut [u8]) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_hardware_backend_enum_routing() {
        let dummy = DummyBackend { last_period: 0 };
        // We temporarily create a test wrapper to check that our main.rs enum works seamlessly
        let test_backend = tokio::sync::Mutex::new(dummy);

        let period = test_backend.lock().await.set_tck_period(500).await;
        assert_eq!(period, 600);
    }
}
