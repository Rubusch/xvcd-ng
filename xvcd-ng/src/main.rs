use clap::Parser;
use xvcd_ng::{CliArgs, BackendMode, FtdiMpsseBackend, FtdiBitbangBackend, JtagController, xvc_server::XvcServer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let args = CliArgs::parse();

    let hardware: Box<dyn JtagController> = match args.mode {
        BackendMode::Mpsse => {
            match FtdiMpsseBackend::new(args.vid, args.pid, args.channel) {
                Ok(hw) => Box::new(hw),
                Err(e) => {
                    eprintln!("Fatal MPSSE Hardware Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BackendMode::Bitbang => {
            match FtdiBitbangBackend::new(args.vid, args.pid, args.channel) {
                Ok(hw) => Box::new(hw),
                Err(e) => {
                    eprintln!("Fatal Bitbang Hardware Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let server = XvcServer::new(args.port).await?;
    println!("xvcd-ng server initialization complete. Listening on port {}...", args.port);

    let static_hw = Box::leak(hardware);
    server.run(static_hw).await
}
