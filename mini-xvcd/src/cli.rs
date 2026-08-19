use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMode {
    Bitbang,
    Mpsse,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "mini-xvcd", version, about = "Xilinx Virtual Cable Daemon in Rust", long_about = None)]
pub struct CliArgs {
    #[arg(short = 'P', long, default_value = "2542")]
    pub port: u16,

    #[arg(short, long, default_value = "0403", value_parser = parse_hex_u16)]
    pub vid: u16,

    #[arg(short, long, default_value = "6010", value_parser = parse_hex_u16)]
    pub pid: u16,

    #[arg(short, long, value_enum, default_value_t = BackendMode::Bitbang)]
    pub mode: BackendMode,

    /// Hardware channel port selection index (0 = Channel A, 1 = Channel B, etc.)
    #[arg(short, long, default_value = "0")]
    pub channel: u8,
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
        let args = CliArgs::try_parse_from(["mini-xvcd"]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 2542);
        assert_eq!(config.vid, 0x0403);
        assert_eq!(config.pid, 0x6010);
        assert_eq!(config.mode, BackendMode::Bitbang);
        assert_eq!(config.channel, 0);
    }

    #[test]
    fn test_cli_custom_hex_and_port_parsing_long() {
        let args = CliArgs::try_parse_from([
            "mini-xvcd",
            "--port", "3000",
            "--vid", "0xabcd",
            "--pid", "1234",
            "--mode", "mpsse",
            "--channel", "1"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.vid, 0xabcd);
        assert_eq!(config.pid, 0x1234);
        assert_eq!(config.mode, BackendMode::Mpsse);
        assert_eq!(config.channel, 1);
    }

    #[test]
    fn test_cli_custom_hex_and_port_parsing_long_hex() {
        let args = CliArgs::try_parse_from([
            "mini-xvcd",
            "--port", "3000",
            "--vid", "0xabcd",
            "--pid", "0x1234",
            "--mode", "mpsse",
            "--channel", "2"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.vid, 0xabcd);
        assert_eq!(config.pid, 0x1234);
        assert_eq!(config.mode, BackendMode::Mpsse);
        assert_eq!(config.channel, 2);
    }

    #[test]
    fn test_cli_custom_hex_and_port_parsing_short() {
        let args = CliArgs::try_parse_from([
            "mini-xvcd",
            "-P", "3000",
            "-v", "0xabcd",
            "-p", "1234",
            "-m", "mpsse",
            "-c", "1"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.vid, 0xabcd);
        assert_eq!(config.pid, 0x1234);
        assert_eq!(config.mode, BackendMode::Mpsse);
        assert_eq!(config.channel, 1);
    }

    #[test]
    fn test_cli_hex_parsing_variations() {
        // Verify both standard raw hex strings and 0x-prefixed variants evaluate identically
        let args_raw = CliArgs::try_parse_from(["mini-xvcd", "--vid", "0403", "--pid", "6011"]);
        let args_prefix = CliArgs::try_parse_from(["mini-xvcd", "--vid", "0x0403", "--pid", "0x6011"]);

        assert!(args_raw.is_ok());
        assert!(args_prefix.is_ok());

        assert_eq!(args_raw.unwrap().vid, 0x0403);
        assert_eq!(args_prefix.unwrap().vid, 0x0403);
    }

    #[test]
    fn test_cli_short_flags_combination() {
        let args = CliArgs::try_parse_from([
            "mini-xvcd",
            "-P", "9000",
            "-v", "0403",
            "-p", "6010",
            "-m", "bitbang",
            "-c", "1"
        ]);
        assert!(args.is_ok());

        let config = args.unwrap();
        assert_eq!(config.port, 9000);
        assert_eq!(config.mode, BackendMode::Bitbang);
        assert_eq!(config.channel, 1);
    }

    #[test]
    fn test_cli_invalid_hex_rejection() {
        let args = CliArgs::try_parse_from(["mini-xvcd", "--vid", "not_a_hex_value"]);
        assert!(args.is_err());

        let err_msg = args.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid hex numeric value"));
    }

    #[test]
    fn test_cli_invalid_port_rejection() {
        let args = CliArgs::try_parse_from(["mini-xvcd", "--port", "70000"]); // Port value out of u16 range
        assert!(args.is_err());
    }

    #[test]
    fn test_cli_invalid_mode_rejection() {
        let args = CliArgs::try_parse_from(["mini-xvcd", "--mode", "unknown_mode"]);
        assert!(args.is_err());
    }
}
