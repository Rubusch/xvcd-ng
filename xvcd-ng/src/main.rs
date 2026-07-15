use clap::Parser;
use std::net::TcpListener;
use xvcd_ng::{CliArgs, FtdiBitbangBackend, FtdiMpsseBackend, JtagController, process_xvc_stream};
use xvcd_ng::cli::BackendMode;

fn main() -> std::io::Result<()> {
    let args = CliArgs::parse();

    println!("Initializing xvcd-ng configuration engine...");
    println!("Target configuration setup -> VID: 0x{:04x}, PID: 0x{:04x}, Mode: {:?}", args.vid, args.pid, args.mode);

    let mut hardware: Box<dyn JtagController> = match args.mode {
        BackendMode::Bitbang => {
            match FtdiBitbangBackend::new(args.vid, args.pid) {
                Ok(hw) => Box::new(hw),
                Err(e) => {
                    eprintln!("Fatal Bitbang Engine Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BackendMode::Mpsse => {
            match FtdiMpsseBackend::new(args.vid, args.pid) {
                Ok(hw) => Box::new(hw),
                Err(e) => {
                    eprintln!("Fatal MPSSE Engine Initialization Failure: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))?;
    println!("xvcd-ng server listening for incoming connections on port {}...", args.port);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                println!("Client connected from remote target: {:?}", s.peer_addr());
                if let Err(e) = process_xvc_stream(s, &mut *hardware) {
                    eprintln!("Error handling connection session: {}", e);
                }
                println!("Client transaction stream terminated.");
            }
            Err(e) => eprintln!("Connection intake failure: {}", e),
        }
    }
    Ok(())
}
