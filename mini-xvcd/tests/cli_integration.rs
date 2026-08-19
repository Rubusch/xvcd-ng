use std::process::Command;

#[test]
fn test_binary_execution_missing_hardware_fault() {
    let binary_path = env!("CARGO_BIN_EXE_mini-xvcd");

    let output = Command::new(binary_path)
        .args(["--vid", "FFFF", "--pid", "FFFF", "--mode", "mpsse"])
        .output()
        .expect("Failed to execute mini-xvcd binary target process");

    // The binary should terminate with an error status (exit code 1) because
    // no physical FTDI hardware device exists matching the vendor ID 0xFFFF
    assert_eq!(output.status.code(), Some(1));

    let stderr_content = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr_content.contains("Fatal MPSSE Hardware Initialization Failure") ||
        stderr_content.contains("Could not locate FTDI chip"),
        "Unexpected stderr output: {}", stderr_content
    );
}

#[test]
fn test_binary_help_flag_output() {
    let binary_path = env!("CARGO_BIN_EXE_mini-xvcd");

    let output = Command::new(binary_path)
        .arg("--help")
        .output()
        .expect("Failed to execute mini-xvcd binary target process");

    // The help flag should exit cleanly with a status code of 0
    assert!(output.status.success());

    let stdout_content = String::from_utf8_lossy(&output.stdout);

    assert!(stdout_content.contains("mini-xvcd"));
    assert!(stdout_content.contains("Xilinx Virtual Cable Daemon in Rust"));
}
