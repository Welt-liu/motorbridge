# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic
Versioning.

## [Unreleased]

## [0.3.4] - 2026-05-20

### Added

- Added a detailed Chinese WebSocket gateway protocol manual covering every
  JSON `op`, parameters, defaults, vendor applicability, responses, and browser
  usage examples.
- Added RobStride Rust/Python CLI manual test commands for read/write/readback,
  optional store, active-report, clear-error, control smoke tests, and ID update
  workflows.
- Added regression coverage for RobStride parameter save responses that return
  a non-status device reply.

### Fixed

- Fixed `ws_gateway` `set_id` so Damiao `--transport dm-serial` is honored
  instead of accidentally opening the SocketCAN/PCAN path.
- Improved Damiao `ensure_control_mode()` so an initial register-10 read timeout
  no longer prevents writing the requested mode; write verification now retries.
- Fixed RobStride `save_parameters()` to accept valid device replies after
  communication type `22`, avoiding false `control ack timeout: comm_type=22`
  errors after a parameter was already written and read back successfully.
- Fixed strict Clippy failures across `motor_core`, `motor_cli`,
  `motor_vendor_robstride`, and `ws_gateway`.

### Changed

- Python package version advanced to `0.3.4`.
- Rust workspace package version advanced to `0.3.4` for release/tag alignment.

## [0.3.3] - 2026-05-19

### Added

- Added `-v` / `--version` output to the Rust CLI and Python CLI.
- Added Python binding version helpers: `motorbridge.__version__` and
  `motorbridge.get_version()`.
- Added `--store 1` to Python `robstride-write-param` and `damiao-write-param`
  subcommands for unified write, verify, and persist workflows.
- Added Rust CLI Damiao `read-param` / `write-param` support with matching
  `--type`, `--verify`, and `--store` semantics.

### Fixed

- Python CLI now disables argparse long-option abbreviation for the root parser,
  subcommands, and legacy run parser. Invalid options such as `--mode save` on
  `robstride-write-param` are rejected instead of being misparsed as `--model save`.
- RobStride `save_parameters()` now waits for the protocol status ACK after
  sending communication type `22`.

### Changed

- Python package version advanced to `0.3.3`.
- Rust workspace package version advanced to `0.3.3` for release/tag alignment.

## [0.3.2] - 2026-05-18

### Added

- Added RobStride fault-report diagnostics to the C ABI and Python SDK via
  `motor_handle_robstride_get_fault_report` / `Motor.robstride_get_fault_report()`.
- Python CLI state printing now includes non-zero RobStride `fault_raw` and
  `warning_raw` values.

### Fixed

- RobStride `FAULT_REPORT` frames no longer overwrite the latest motion state.
  Fault reports now update only the fault cache, so fault payloads are not
  exposed as bogus `-720 deg` / `-50 rad/s` / `0 C` feedback.
- RobStride `FAULT_REPORT` frames no longer advance the control status ACK
  sequence, avoiding false command acknowledgements.
- RobStride `clear_error()` clears the local cached fault report only after the
  device acknowledges the clear request.

### Changed

- Python package version advanced to `0.3.2`.
- Rust workspace package version advanced to `0.3.2` for release/tag alignment.

## [0.3.1] - 2026-05-15

### Fixed

- Python CLI RobStride `mit`, `pos-vel`, and `vel` now align their control
  startup sequence with the Rust CLI and WebSocket gateway: disable torque,
  set and verify `run_mode` via `0x7005`, re-enable torque, then send targets.
  This fixes cases where direct Python CLI `pos-vel` could enable the motor but
  fail to move until a Rust CLI or gateway scan/control path had prepared the mode.

### Changed

- Python package version advanced to `0.3.1`.
- Rust workspace package version advanced to `0.3.1` for release/tag alignment.

## [0.3.0] - 2026-05-15

### Added

- RobStride clear-fault support is now exposed through the unified clear-error path.
  `motor_handle_clear_error` sends RobStride communication type `4` with `data[0]=1`.
- RobStride active-report support is now exposed across Rust CLI, Python CLI/SDK,
  ABI, and WebSocket gateway.
- New ABI symbol: `motor_handle_robstride_set_active_report`.
- New Python SDK method: `Motor.robstride_set_active_report(enabled)`.
- New WS gateway operation: `{"op":"set_active_report","enabled":true}`.
- RobStride communication type `21` fault reports are decoded into raw fault/warning
  words plus documented fault and warning bits for diagnostics.

### Documentation

- Added RobStride bring-up notes for `EPScan_time(0x7026)`, including
  `EPScan_time=3` as the recommended initial 20 ms report interval for arm calibration.
- Added CLI, Python, ABI, and WS examples for RobStride clear-error and active-report.

## [0.2.9] - 2026-05-14

### Added

- Python CLI now exposes Damiao parameter/register read and write commands:
  `damiao-read-param` and `damiao-write-param`.
- Python CLI `run` now accepts Rust-style RobStride parameter modes:
  `--mode read-param`, `--mode write-param`, and `--mode save`.
- Python CLI `run` now accepts Rust-style ID update shortcuts:
  `--set-motor-id`, `--set-feedback-id`, `--verify-id`, and Damiao model verification options.

### Fixed

- Legacy Python CLI flat commands such as `motorbridge-cli --vendor robstride ...`
  are parsed as `run` commands instead of being rejected as invalid subcommands.
- Python CLI RobStride `pos-vel` now follows the same native register path as the
  WS gateway (`limit_spd` `0x7017`, `loc_kp` `0x701E`, `loc_ref` `0x7016`).
- RobStride and Damiao Python CLI documentation was aligned with the Rust CLI and binding behavior.

### Changed

- Python binding package version advanced to `0.2.9`.
- Rust workspace crates advanced to `0.2.9`.

## [0.2.8] - 2026-05-12

### Fixed

- Restored the MotorBridge tree to the v0.2.6-compatible unified RobStride interface shape.
- Completed the RobStride protocol section 4 runtime parameter list through `0x702E`,
  including `damper`, `add_offset`, `alveolous_open`, `iq_test`, and `dcc_set`.
- RobStride `set_zero_position()` now keeps the same upper-level command/API shape while
  writing `zero_sta(0x7029)=1` behind the scenes so zeroed motors use the `-pi..pi`
  startup coordinate range.
- RobStride parameter save now sends the official type-22 payload `01 02 03 04 05 06 07 08`.

### Changed

- Python binding package version advanced to `0.2.8`.
- Rust workspace crates advanced to `0.2.8`.

## [0.2.6] - 2026-05-09

### Added

- RobStride host-id-specific ABI helpers for exact scan probing from Python:
  `motor_handle_robstride_ping_host_id` and `motor_handle_robstride_get_param_f32_host_id`.
- Python SDK wrappers `robstride_ping_host_id(...)` and `robstride_get_param_f32_host_id(...)`.
- Release test note `release_test_notes/0.2.6.md` covering Rust core/CLI, Python binding/CLI,
  package smoke checks, and full RobStride/Damiao CLI command examples.

### Changed

- RobStride `motor_id` / `device_id` is now validated as `1..255`; `feedback_id` / `host_id` is
  validated as `0..255` across core, Rust CLI, Python SDK/CLI, and websocket gateway flows.
- Rust and Python RobStride scan now probe each listed `--feedback-ids` host ID exactly instead of
  silently falling back inside each candidate probe.
- RobStride parameter response filtering now requires the response `device_id` to match the target
  motor, reducing cross-talk risk on multi-motor buses.
- The embedded `bindings/python/mintlify` documentation copy was removed; canonical Mintlify docs now
  live in the sibling `motorbridge-docs` repository.

## [0.2.5] - 2026-05-09

### Added

- Python CLI `id-set --vendor robstride` now supports RobStride device ID updates with optional store and verify.
- Rust `motor_cli` accepts Python-style bare mode shorthand, for example `motor_cli scan --vendor robstride ...`.
- Rust RobStride scan now accepts `--feedback-ids`, `--timeout-ms`, `--param-id`, and `--param-timeout-ms`, matching the Python scan entrypoint.

### Changed

- RobStride scan output and documentation now consistently distinguish motor `device_id` / `probe` from host-side `feedback_id` / `host_id`.
- Python and Rust RobStride scan defaults are aligned around host ID candidates `0xFD,0xFF,0xFE,0x00,0xAA`.

## [0.2.3] - 2026-04-16

### Changed

- Refactored ABI FFI layers to reduce duplicated controller/motor dispatch boilerplate via shared
  macros and helpers.
- Consolidated vendor parameter FFI entrypoints (Hexfellow/HighTorque/MyActuator/RobStride) with
  shared macro-generated get/write wrappers.
- Aligned runtime/control-path robustness fixes across motor core, vendor controllers, Python
  bindings, and websocket gateway integration.

## [0.1.3] - 2026-03-24

### Added

- New practical Damiao guide:
  - `examples/damiao_controll_all_in_one.md`
  - includes one-page command bundles for:
    - CLI four core modes (`mit`, `pos-vel`, `vel`, `force-pos`)
    - C/C++ ABI examples
    - Python ctypes ABI examples
    - Python bindings examples
    - C++ bindings examples

### Changed

- Damiao CLI runtime output (`motor_cli/src/damiao_cli.rs`) now prints richer realtime fields:
  - `id`, `arbitration_id`, `status_name`
  - temperatures `t_mos`, `t_rotor`
  - mode-aware command/target context and tracking errors
    - MIT: `cmd_pos/cmd_vel/kp/kd/cmd_tau/e_pos/e_vel`
    - POS_VEL: `cmd_pos/vlim/e_pos`
    - VEL: `cmd_vel/e_vel`
    - FORCE_POS: `cmd_pos/vlim/ratio/e_pos`

## [0.1.2] - 2026-03-23

### Changed

- Release version bump from `0.1.1` to `0.1.2` for clean tag progression.
- Damiao `dm-serial` documentation rollout remains aligned across:
  - CLI README (full interface section)
  - root README
  - bindings/examples/integrations/tools related READMEs.

## [0.1.1] - 2026-03-23

### Added

- Damiao serial-bridge transport (`dm-serial`) for unix-like systems:
  - CLI transport selection: `--transport auto|socketcan|dm-serial`
  - Serial options: `--serial-port`, `--serial-baud`
  - Damiao controller serial constructor and transport runtime wiring.
- C ABI constructor for Damiao serial bridge:
  - `motor_controller_new_dm_serial(serial_port, baud)`
- SDK support for Damiao serial bridge:
  - Python: `Controller.from_dm_serial(...)`
  - C++: `Controller::from_dm_serial(...)`
- New Chinese operation manual for deployment/runtime usage:
  - `docs/zh/operation_manual.md`

### Changed

- README alignment across examples/bindings/integrations/tools:
  - All Damiao-related READMEs now mention `dm-serial` availability.
  - Added explicit pointer to complete interface/command section in
    `motor_cli/README.zh-CN.md` (`3.6`) and `motor_cli/README.md`.

## [0.1.0] - 2026-03-20

### Added

- Linux CANable candleLight/gs_usb quick guide in root README (EN/ZH), including candleLight/gs_usb setup and
  `--channel can0` usage examples.
- Channel quick reference in `motor_cli/README.md` and `motor_cli/README.zh-CN.md` covering:
  - Linux SocketCAN channels (`can0`, `can1`) and Linux rule "no `@bitrate` in channel name"
  - Windows PCAN channel mapping (`can0/can1`) with optional `@bitrate`

### Changed

- CLI startup summary now distinguishes scan semantics from control semantics:
  - `--mode scan` prints `model_hint`, `base_feedback_id`, and `scan_range`
  - defaults are explicitly tagged as `(default)` to reduce confusion

### Fixed

- RobStride frame filtering now only accepts status/fault frames from the target motor ID,
  preventing cross-device state pollution on shared CAN buses.
- Architecture Mermaid diagrams (EN/ZH) now include `myactuator` branch for consistency with
  workspace/runtime layout.

### Usage

- Linux CANable candleLight/gs_usb setup and examples:
  - `README.md` / `README.zh-CN.md` section: "Linux CANable candleLight/gs_usb Quick Guide"
- Channel compatibility and parameter rules:
  - `motor_cli/README.md` / `motor_cli/README.zh-CN.md` section: "Channel Quick Reference"
