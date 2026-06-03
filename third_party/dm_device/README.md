# DM Device SDK Third-Party Runtime

This directory vendors the DaMiao DM_Device SDK binaries used by the
`dm-device` transport.

It exists so motorbridge can build, run, and package the USB/CAN adapter
support without depending on a developer's local copy of
`dm-device-sdk/C&C++/lib/v1.1.0`.

## Contents

`v1.1.0/dmcan.h`

- C/C++ SDK header from DaMiao.
- Used at build time by `motor_core/src/dm_device_shim.cpp`.

`v1.1.0/linux/*/libdm_device.so`

- Linux runtime libraries for the supported CPU architectures.

`v1.1.0/macos/*/libdm_device.dylib`

- macOS runtime libraries for the supported CPU architectures.

`v1.1.0/windows/*`

- Windows runtime/import libraries for MSVC and MinGW builds.

## User Runtime Setup

Python wheels do not embed these vendor runtime libraries. When a user runs
`Controller.from_dm_device(...)`, `motorbridge-cli --transport dm-device`, or
`motorbridge-gateway --transport dm-device`, motorbridge first tries to find the
matching runtime in these places:

1. `MOTOR_DM_DEVICE_LIB=/absolute/path/to/<runtime>`
2. A source checkout path:
   `third_party/dm_device/v1.1.0/<platform>/<arch>/<runtime>`
3. A user cache path:
   `~/.cache/motorbridge/dm_device/v1.1.0/<platform>/<arch>/<runtime>`
   or `$XDG_CACHE_HOME/motorbridge/dm_device/v1.1.0/...`

If the file is missing, motorbridge prints the required runtime path and a
download URL. The canonical download page is:

```text
https://github.com/motorbridge/motorbridge/tree/main/third_party/dm_device
```

Example for Linux x86_64:

```bash
mkdir -p ~/.cache/motorbridge/dm_device/v1.1.0/linux/x86_64
curl -L \
  -o ~/.cache/motorbridge/dm_device/v1.1.0/linux/x86_64/libdm_device.so \
  https://raw.githubusercontent.com/motorbridge/motorbridge/main/third_party/dm_device/v1.1.0/linux/x86_64/libdm_device.so

# Or point directly at a manually downloaded SDK file:
export MOTOR_DM_DEVICE_LIB=/absolute/path/to/libdm_device.so
```

The helper command prints the same guidance:

```bash
motorbridge-install-dm-device
```

It downloads only when explicitly requested:

```bash
motorbridge-install-dm-device --download
```

## Platform Support Rule

`dm-device` support is intentionally tied to the SDK runtime libraries that are
actually present in this directory. During `motor_core` build, `build.rs` maps
the Rust target platform to one expected SDK runtime path:

| Rust target | SDK runtime path |
|---|---|
| `x86_64-unknown-linux-*` | `linux/x86_64/libdm_device.so` |
| `aarch64-unknown-linux-*` | `linux/arm64/libdm_device.so` |
| `aarch64-apple-darwin` | `macos/arm64/libdm_device.dylib` |
| `x86_64-apple-darwin` | `macos/x86_64/libdm_device.dylib` |
| `x86_64-pc-windows-msvc` | `windows/msvc/dm_device.dll` |
| `x86_64-pc-windows-gnu` | `windows/mingw/libdm_device.dll` |

If the mapped file exists, motorbridge compiles the C++ shim and enables the
real `DmDeviceBus`. If the mapped file is missing, motorbridge still builds for
that target, but `--transport dm-device` reports that DM_Device SDK is not
bundled for the platform. In other words, adding/removing SDK runtime files here
is what controls which architectures support `dm-device`.

## Support Matrix

| Platform / Architecture | Runtime Path | Python Wheel | `dm-device` Runtime | Hardware Verified |
|---|---|---|---|---|
| Linux x86_64 | `v1.1.0/linux/x86_64/libdm_device.so` | yes | supported | yes, USB2CANFD_DUAL CANFD1/CANFD2 scan |
| Linux aarch64 / arm64 | `v1.1.0/linux/arm64/libdm_device.so` | yes | supported | pending host validation |
| Windows x86_64 MSVC | `v1.1.0/windows/msvc/dm_device.dll` | yes | supported | pending host validation |
| Windows x86_64 MinGW | `v1.1.0/windows/mingw/libdm_device.dll` | ABI/CLI build support | supported | pending host validation |
| macOS arm64 | `v1.1.0/macos/arm64/libdm_device.dylib` | yes | supported | pending host validation |
| macOS x86_64 | `v1.1.0/macos/x86_64/libdm_device.dylib` | no official wheel | source/manual install only | pending host validation |
| Other OS/arch | none | no | unsupported | unsupported |

## Relationship To Motorbridge

The integration has three layers:

1. `third_party/dm_device`

   Stores the vendor SDK header and platform runtime libraries.

2. `motor_core/src/dm_device_shim.cpp`

   A small C++ shim that talks directly to the DaMiao SDK. It dynamically
   loads `libdm_device.so`/`.dylib`/`.dll`, opens the adapter, configures
   channels, sends CAN frames, receives SDK callbacks, and exposes a small C
   ABI (`mb_dm_open`, `mb_dm_send`, `mb_dm_recv`, `mb_dm_shutdown`).

3. `motor_core/src/dm_device.rs`

   The Rust transport wrapper. It calls the shim C ABI and implements
   motorbridge's `CanBus` trait, so vendor controllers can use DM_Device in
   the same style as SocketCAN, CAN-FD, or `dm-serial`.

After that, vendor code such as the Damiao controller can simply open:

```text
--transport dm-device
--dm-device-type usb2canfd-dual
--dm-channel canfd1
```

or:

```text
--dm-channel canfd2
```

For `motor_cli --mode scan`, omit `--dm-channel` on `usb2canfd-dual` to scan
both CANFD1 and CANFD2. Add `--dm-channel canfd1` or `--dm-channel canfd2`
only when you want to scan one physical channel.

## Why A C++ Shim Exists

The DaMiao SDK header exposes C++-shaped types and callback structs. In
particular, the receive frame layout uses SDK-defined structs/bitfields. That
layout is fragile to reproduce directly in Rust FFI.

The C++ shim keeps the SDK boundary in C++, where `dmcan.h` is native and the
frame layout is interpreted by the compiler that sees the real SDK header.
Rust then talks only to a small, stable C ABI with plain types:

- integers
- byte arrays
- pointers
- fixed-size frame structs

This makes the Rust side simpler and safer.

The shim also mirrors the known-good C++ diagnostic tool behavior. On Linux,
the SDK/libusb teardown path was observed to leave the USB adapter in a bad
state or print repeated `libusb_transfer_cancelled` messages. For that reason,
`mb_dm_shutdown` unregisters motorbridge's callback/queue state but deliberately
does not call the SDK close/destroy functions.

## Current Scope

Currently supported through motorbridge:

- `usb2canfd`
- `usb2canfd-dual`
- `linkx4c` parsing/open path
- `canfd1` and `canfd2` channel selection
- classic CAN frames up to 8 bytes
- standard and extended CAN identifiers

Known current limits:

- The active implementation sends classic CAN frames, not 64-byte CAN-FD
  payloads.
- `usb2canfd-dual` opens/configures both physical channels, then filters RX/TX
  to the selected `--dm-channel`.
- The first matching DM_Device adapter is opened; there is not yet a serial
  number/device-index selector.
- The SDK appears to be exclusive per USB adapter. Do not open the same
  USB2CANFD_DUAL from two motorbridge processes at the same time.

## Packaging

Python packaging intentionally does not copy `libdm_device.so`/`.dylib`/`.dll`
into wheels. This avoids manylinux `auditwheel` failures caused by the vendor
Linux library's newer `GLIBCXX` dependency, and keeps the vendor runtime setup
explicit.

For development or diagnostics, `MOTOR_DM_DEVICE_LIB` can be set manually to
override the library path. Source-tree runs also resolve files placed under
`third_party/dm_device/v1.1.0/...`.

## Updating The SDK

When updating DaMiao DM_Device SDK:

1. Add the new SDK files under a new version directory, for example
   `third_party/dm_device/v1.2.0`.
2. Update `motor_core/build.rs` include path if the header version changes.
3. Update `motor_core/src/dm_device.rs` runtime search paths.
4. Rebuild `motor_core`, `motor_abi`, and `motor_cli`.
5. Test both channels on USB2CANFD_DUAL:

```text
cargo run -p motor_cli -- --vendor damiao --transport dm-device --dm-device-type usb2canfd-dual --model 4310 --mode scan --start-id 1 --end-id 16
cargo run -p motor_cli -- --vendor damiao --transport dm-device --dm-device-type usb2canfd-dual --dm-channel canfd1 --model 4310 --mode scan --start-id 1 --end-id 16
cargo run -p motor_cli -- --vendor damiao --transport dm-device --dm-device-type usb2canfd-dual --dm-channel canfd2 --model 4310 --mode scan --start-id 1 --end-id 16
```
