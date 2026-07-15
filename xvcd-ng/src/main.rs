use clap::Parser;
use std::net::TcpListener;
use xvcd_ng::{CliArgs, FtdiMpsseBackend, process_xvc_stream};

fn main() -> std::io::Result<()> {
    let args = CliArgs::parse();

    let mut hardware = match FtdiMpsseBackend::new(args.vid, args.pid, args.channel) {
        Ok(hw) => hw,
        Err(e) => {
            eprintln!("Fatal Hardware Discovery Initialization Failure: {}", e);
            std::process::exit(1);
        }
    };

    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))?;
    println!("xvcd-ng server listening for incoming connections on port {}...", args.port);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                println!("Client connected from remote target context: {:?}", s.peer_addr());
                if let Err(e) = process_xvc_stream(s, &mut hardware) {
                    eprintln!("Session processing encountered network error: {}", e);
                }
                println!("Client transaction stream terminated.");
            }
            Err(e) => eprintln!("Inbound connection intake failure: {}", e),
        }
    }
    Ok(())
}
