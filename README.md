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

## mini-xvcd
### Build
Prepare for crosscompiling
```
$ mkdir .cargo
$ vi .cargo/config.toml
    [target.aarch64-unknown-linux-gnu]
    linker = "aarch64-linux-gnu-gcc"
```

Build for RPi4 (aarch64)
```
$ cargo build --target aarch64-unknown-linux-gnu --release
```

Find the binary here: `./target/aarch64-unknown-linux-gnu/release/mini-xvcd`

## Installation
Adjust the provided systemd service file and place it under `/etc/systemd/system/mini-xvcd.servic`

Then simply start/stop or enable automatic start up with
```
# Ingest the new service file
sudo systemctl daemon-reload

# Enable the background process to start at boot
sudo systemctl enable xvcd.service

# Start the XVC daemon right now
sudo systemctl start xvcd.service
```

## Usage
Start the mini-xvcd manually to get the most recent help. Find example usage in the systemd service file.
### Manual start
In case unload some modules to avoid being busy locked out.
```
sudo rmmod ftdi_sio
sudo rmmod usbserial
sudo ./mini-xvcd
```

see the help
```
$ sudo ./mini-xvcd --help
Xilinx Virtual Cable Daemon in Rust

Usage: mini-xvcd [OPTIONS]

Options:
  -P, --port <PORT>        [default: 2542]
  -v, --vid <VID>          [default: 0403]
  -p, --pid <PID>          [default: 6010]
  -m, --mode <MODE>        [default: mpsse] [possible values: bitbang, mpsse]
  -c, --channel <CHANNEL>  Hardware channel port selection index (0 = Channel A, 1 = Channel B, etc.) [default: 0]
  -h, --help               Print help
  -V, --version            Print version
```
