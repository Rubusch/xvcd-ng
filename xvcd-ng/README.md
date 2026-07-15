# Development Notes

building for the RPi4
```
$ rustup target add aarch64-unknown-linux-gnu
$ CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --target aarch64-unknown-linux-gnu --release
```

- We scaled the command detection buffer down from 16 bytes to 8 bytes because XVC commands are exactly 8 bytes long.

## Hardware Setup
Hardware Configuration: FT2232H
- Vendor ID (VID): 0x0403
- Product ID (PID): 0x6010 (Standard FT2232C/D/H series)
- RPi4: controller board for device under test (DUT)

## Crosscompiling

### rusb
Build for RPi4 (aarch64)
```
$ cargo build --target aarch64-unknown-linux-gnu --release
```
Find the binary here: `./target/aarch64-unknown-linux-gnu/release/xvcd-ng`
