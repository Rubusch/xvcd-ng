# Development Notes

building for the RPi4
```
$ rustup target add aarch64-unknown-linux-gnu
$ CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --target aarch64-unknown-linux-gnu --release
```

- We scaled the command detection buffer down from 16 bytes to 8 bytes because XVC commands are exactly 8 bytes long.

## Setup
Hardware Configuration: FT2232H
- Vendor ID (VID): 0x0403
- Product ID (PID): 0x6010 (Standard FT2232C/D/H series)

- First approach: asynchronous bit banging JTAG, replaceable by MPSSE approach -> trait based generics setup
