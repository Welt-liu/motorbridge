# MotorBridge Scripts

Helper scripts for preparing CAN interfaces and release workflows.

## PCAN: `can_restart.sh`

Use `can_restart.sh` for PEAK PCAN-USB adapters exposed as Linux SocketCAN
interfaces such as `can0` or `can1`.

```bash
scripts/can_restart.sh
scripts/can_restart.sh can0
scripts/can_restart.sh --bitrate 1000000 can0
```

Defaults:

- bitrate: `1000000`
- bus-off auto-restart: `restart-ms 100`
- loopback: `off`
- tx queue length: `2000`
- interfaces: `can0 can1`

PCAN-USB verified healthy output:

```text
can state ERROR-ACTIVE (berr-counter tx 0 rx 0) restart-ms 100
bitrate 1000000
pcan_usb
```

That means PCAN is up, running at 1 Mbps, has no current CAN error count, and
bus-off auto-restart is enabled.

Recommended alias:

```bash
alias can_restart='/home/w0x7ce/Downloads/MOTOR_LIB/motorbridge/scripts/can_restart.sh'
```

## SLCAN: `slcan_restart.sh`

Use `slcan_restart.sh` only for serial-line CAN adapters that appear as
`/dev/ttyACM0` or `/dev/ttyUSB0` and need `slcand`.

```bash
scripts/slcan_restart.sh /dev/ttyACM0 can0 1000000
```

## Other helpers

- `canfd_restart.sh`: prepare CAN-FD capable SocketCAN interfaces.
- `setup_pcbusb_macos.sh`: install the macOS PCBUSB runtime for PCAN support.
