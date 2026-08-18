use clap::Parser;
use xvcd_ng::{CliArgs, BackendMode, FtdiMpsseBackend, FtdiBitbangBackend, xvc_server::XvcServer};

enum HardwareBackend {
    Mpsse(FtdiMpsseBackend),
    Bitbang(FtdiBitbangBackend),
}

impl xvcd_ng::JtagController for HardwareBackend {
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
    println!("xvcd-ng server initialization complete. Listening on port {}...", args.port);

    server.run(&mut hardware).await
}
