[![License: MIT](https://shields.io)](https://opensource.org/licenses/MIT)

# Lothar's Mini-XVCD (Xilinx Virtual Cable Daemon)
Inspired by the idea of https://github.com/tmbinc/xvcd, this is my mini-xvcd implementation approach. I
needed this on RPi4 for remote JTAG debugging and access for basic register readouts using XVD over USB
connected FTDI/JTAG. In contrast to the original xvcd, my implementation supports faster MPSSE (default),
but also slow bitbanging mode.

In particular, XVC (Xilinx Virtual Cable) is based only on `getinfo`, `settck` and `shift`. While the
hw-server implements the full TCF (Target Communication Framework) as a proprietary binary. While writing
registers, downloading bitstreams, debugging ILA cores, etc. should work perfectly, limitations are e.g.
burning eFUSES where Xilinx requires a direct hw-server connection.

## Hardware Setup
Hardware Configuration: FT2232H
- Vendor ID (VID): 0x0403
- Product ID (PID): 0x6010 (Standard FT2232C/D/H series)
- RPi4: controller board for device under test (DUT)

## Build
Prepare for crosscompiling (linux /debian)

Download the C-based toolchain. Note, in principle the source could be compiled with e.g. a pure Rust
port of musl, since main crates are nusb and ftdi-nusb, but since this is still experimental.
Therefore at the moment, safest is the following:
```
$ sudo apt update && sudo apt install -y gcc-aarch64-linux-gnu
```

Prepare Rust or verify
```
$ rustup target add aarch64-unknown-linux-gnu

$ mkdir .cargo
$ vi .cargo/config.toml
    [target.aarch64-unknown-linux-gnu]
    linker = "aarch64-linux-gnu-gcc"
```

Build for RPi4 (aarch64)
```
$ cargo build --target aarch64-unknown-linux-gnu --release
```

Find the binary: `./target/aarch64-unknown-linux-gnu/release/mini-xvcd`

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
In case unload some modules to avoid being busy locked out, anyway the implementation will unload them,
if in the way.
```
$ sudo rmmod ftdi_sio
$ sudo rmmod usbserial
$ sudo ./mini-xvcd
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

## Troubleshooting
### issue: `DR shift through all zeroes`
e.g. in xsdb after connecting...
```
tcfchan#0
xsdb% tar
  1  whole scan chain (DR shift through all zeroes)
```
**fix**: Is the device powered?

### issue: `device configuration unstable`
mini-xvcd started in mode _bitbang_, then
```
tcfchan#0
xsdb% tar
  1  whole scan chain (device configuration unstable)
```

**fix**: Use _mpsse_ mode, since it's more stable faster anyway. Unsure, I've seen this from time to time, related to the toolchain / library setup. I could run a version compiled on
the one rust setup pretty stable and never saw that, where another Rust setup was not able to compile it to get _bitbang_ mode stable. I dumped a `cargo tree` of the working setup
under ./docs.

### issue: could not get device
When starting `mini-xvcd`, stops right away with something like could not get device.  

**fix**: Do you pass the correct VID and PID for the device? Is the device around (powered)? Check with `lsusb` what you see, you should see the FTDI along with the `VID:PID` in use, pass
them according to the CLI args to the program and try again.
