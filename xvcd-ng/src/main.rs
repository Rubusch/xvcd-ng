use std::net::TcpListener;
use xvcd_ng::{FtdiBitbangBackend, process_xvc_stream};

const DEFAULT_PORT: &str = "2542";

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", DEFAULT_PORT))?;
    println!("xvcd-ng listening on port {}...", DEFAULT_PORT);

    let mut hardware = match FtdiBitbangBackend::new(0x0403, 0x6010) {
        Ok(hw) => hw,
        Err(e) => {
            eprintln!("Hardware Init Fatal Error: {}", e);
            std::process::exit(1);
        }
    };

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                println!("Client connected from remote target: {:?}", s.peer_addr());
                if let Err(e) = process_xvc_stream(s, &mut hardware) {
                    eprintln!("Error handling connection session: {}", e);
                }
                println!("Client transaction stream terminated.");
            }
            Err(e) => eprintln!("Connection intake failure: {}", e),
        }
    }
    Ok(())
}

