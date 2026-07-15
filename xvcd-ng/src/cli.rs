use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMode {
    Bitbang,
    Mpsse,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "xvcd-ng", version, about = "Xilinx Virtual Cable Daemon in Rust", long_about = None)]
pub struct CliArgs {
    #[arg(short = 'P', long, default_value = "2542")]
    pub port: u16,

    #[arg(short, long, default_value = "0403", value_parser = parse_hex_u16)]
    pub vid: u16,

    #[arg(short, long, default_value = "6010", value_parser = parse_hex_u16)]
    pub pid: u16,

    #[arg(short, long, value_enum, default_value_t = BackendMode::Bitbang)]
    pub mode: BackendMode,
}

fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let clean = s.strip_prefix("0x").unwrap_or(s);
    u16::from_str_radix(clean, 16).map_err(|e| format!("Invalid hex numeric value '{}': {}", s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_default_argument_fallback() {
        let args = CliArgs::try_parse_from(["xvcd-ng"]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 2542);
        assert_eq!(config.vid, 0x0403);
        assert_eq!(config.pid, 0x6010);
        assert_eq!(config.mode, BackendMode::Bitbang);
    }

    #[test]
    fn test_cli_custom_hex_and_port_parsing_long() {
        let args = CliArgs::try_parse_from([
            "xvcd-ng",
            "--port", "3000",
            "--vid", "0xabcd",
            "--pid", "1234",
            "--mode", "mpsse"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.vid, 0xabcd);
        assert_eq!(config.pid, 0x1234);
        assert_eq!(config.mode, BackendMode::Mpsse);
    }

    #[test]
    fn test_cli_custom_hex_and_port_parsing_short() {
        let args = CliArgs::try_parse_from([
            "xvcd-ng",
            "-P", "3000",
            "-v", "0xabcd",
            "-p", "1234",
            "-m", "mpsse"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.vid, 0xabcd);
        assert_eq!(config.pid, 0x1234);
        assert_eq!(config.mode, BackendMode::Mpsse);
    }

    #[test]
    fn test_cli_invalid_hex_rejection() {
        let args = CliArgs::try_parse_from(["xvcd-ng", "--vid", "invalid_hex_string"]);
        assert!(args.is_err()); // Ensure clap rejects malformed hex inputs gracefully
    }
}
