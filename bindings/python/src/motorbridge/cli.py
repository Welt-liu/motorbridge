from __future__ import annotations

import argparse
import sys
import time
from typing import Any

from .core import Controller
from .models import Mode
from .platform_hints import preflight_can_runtime

DAMIAO_MODEL_LIMITS: dict[str, tuple[float, float, float]] = {
    "3507": (12.566, 50.0, 5.0),
    "4310": (12.5, 30.0, 10.0),
    "4310P": (12.5, 50.0, 10.0),
    "4340": (12.5, 10.0, 28.0),
    "4340P": (12.5, 10.0, 28.0),
    "6006": (12.5, 45.0, 20.0),
    "8006": (12.5, 45.0, 40.0),
    "8009": (12.5, 45.0, 54.0),
    "10010L": (12.5, 25.0, 200.0),
    "10010": (12.5, 20.0, 200.0),
    "H3510": (12.5, 280.0, 1.0),
    "G6215": (12.5, 45.0, 10.0),
    "H6220": (12.5, 45.0, 10.0),
    "JH11": (12.5, 10.0, 12.0),
    "6248P": (12.566, 20.0, 120.0),
}


def _mode_to_enum(mode: str) -> Mode:
    return {
        "mit": Mode.MIT,
        "pos-vel": Mode.POS_VEL,
        "vel": Mode.VEL,
        "force-pos": Mode.FORCE_POS,
    }[mode]


def _parse_id(text: str) -> int:
    return int(text, 0)


def _parse_rids(text: str) -> list[int]:
    return [int(x.strip(), 0) for x in text.split(",") if x.strip()]


def _infer_robstride_param_type(param_id: int, explicit_type: str = "") -> str:
    if explicit_type:
        return explicit_type
    # Match the Rust CLI's most common RobStride parameter typing.
    if param_id == 0x7005:
        return "i8"
    if param_id == 0x7029:
        return "u8"
    if param_id == 0x7028:
        return "u32"
    return "f32"


def _read_robstride_param(motor: Any, param_id: int, param_type: str, timeout_ms: int) -> int | float:
    if param_type == "i8":
        return motor.robstride_get_param_i8(param_id, timeout_ms)
    if param_type == "u8":
        return motor.robstride_get_param_u8(param_id, timeout_ms)
    if param_type == "u16":
        return motor.robstride_get_param_u16(param_id, timeout_ms)
    if param_type == "u32":
        return motor.robstride_get_param_u32(param_id, timeout_ms)
    return motor.robstride_get_param_f32(param_id, timeout_ms)


def _write_robstride_param(motor: Any, param_id: int, param_type: str, raw_value: str) -> None:
    if param_type == "i8":
        motor.robstride_write_param_i8(param_id, int(raw_value, 0))
    elif param_type == "u8":
        motor.robstride_write_param_u8(param_id, int(raw_value, 0))
    elif param_type == "u16":
        motor.robstride_write_param_u16(param_id, int(raw_value, 0))
    elif param_type == "u32":
        motor.robstride_write_param_u32(param_id, int(raw_value, 0))
    else:
        motor.robstride_write_param_f32(param_id, float(raw_value))


def _robstride_mode_expect(mode: str) -> tuple[Mode, int, str]:
    if mode == "mit":
        return Mode.MIT, 0, "mit"
    if mode == "pos-vel":
        return Mode.POS_VEL, 1, "pos-vel"
    if mode == "vel":
        return Mode.VEL, 2, "vel"
    raise ValueError(f"RobStride mode guard does not support {mode}")


def _ensure_robstride_mode_for_control(
    ctrl: Controller,
    motor: Any,
    mode: str,
    timeout_ms: int,
    strict: bool,
) -> None:
    target_mode, expect, mode_name = _robstride_mode_expect(mode)
    try:
        actual = motor.robstride_get_param_i8(0x7005, 120)
        if actual == expect:
            return
    except Exception:
        actual = None

    # Match Rust CLI / WS gateway sequencing. Some RobStride firmware variants
    # ignore run_mode writes while torque is enabled.
    try:
        ctrl.disable_all()
    except Exception as e:
        if strict:
            raise
        print(f"[warn] robstride pre-mode disable_all failed: {e}; continue")
    time.sleep(0.06)

    last_error: Exception | None = None
    for _ in range(3):
        try:
            motor.ensure_mode(target_mode, timeout_ms)
            time.sleep(0.03)
            actual = motor.robstride_get_param_i8(0x7005, 120)
            if actual == expect:
                return
        except Exception as e:
            last_error = e
        time.sleep(0.03)

    msg = (
        f"robstride {mode_name} mode switch failed: expect={expect} "
        f"actual={actual!r} error={last_error}"
    )
    if strict:
        raise RuntimeError(msg)
    print(f"[warn] {msg}; continue anyway")


def _match_damiao_models_by_limits(pmax: float, vmax: float, tmax: float, tol: float) -> list[str]:
    return [
        model
        for model, (mp, mv, mt) in DAMIAO_MODEL_LIMITS.items()
        if abs(mp - pmax) <= tol and abs(mv - vmax) <= tol and abs(mt - tmax) <= tol
    ]


def _suggest_damiao_models_by_limits(pmax: float, vmax: float, tmax: float, top_n: int = 3) -> list[str]:
    scored = sorted(
        (
            ((mp - pmax) ** 2 + (mv - vmax) ** 2 + (mt - tmax) ** 2, model)
            for model, (mp, mv, mt) in DAMIAO_MODEL_LIMITS.items()
        )
    )
    return [model for _, model in scored[:top_n]]


def _verify_damiao_model(motor: Any, model: str, timeout_ms: int, tol: float) -> None:
    expected = DAMIAO_MODEL_LIMITS.get(model)
    if expected is None:
        raise ValueError(f"unknown Damiao model in catalog: {model}")
    pmax = motor.get_register_f32(21, timeout_ms)
    vmax = motor.get_register_f32(22, timeout_ms)
    tmax = motor.get_register_f32(23, timeout_ms)
    matched = _match_damiao_models_by_limits(pmax, vmax, tmax, tol)
    if model in matched:
        print(
            f"[ok] model handshake passed: --model {model} "
            f"matches PMAX/VMAX/TMAX=({pmax:.3f}, {vmax:.3f}, {tmax:.3f})"
        )
        return
    suggested = ", ".join(_suggest_damiao_models_by_limits(pmax, vmax, tmax)) or "none"
    raise RuntimeError(
        f"model handshake mismatch: --model {model} expects "
        f"({expected[0]:.3f}, {expected[1]:.3f}, {expected[2]:.3f}), "
        f"device reports ({pmax:.3f}, {vmax:.3f}, {tmax:.3f}), "
        f"suggested: {suggested}. If intentional, run with --verify-model 0"
    )


def _robstride_device_id(value: int, name: str) -> int:
    if not 1 <= value <= 255:
        raise ValueError(f"RobStride {name} must be in 1..255, got {value}")
    return value


def _robstride_host_id(value: int, name: str) -> int:
    if not 0 <= value <= 255:
        raise ValueError(f"RobStride {name}/host_id must be in 0..255, got {value}")
    return value


def _add_common_args(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--vendor",
        default="damiao",
        choices=["damiao", "myactuator", "robstride", "hightorque", "hexfellow"],
        help="motor vendor/protocol family, default damiao",
    )
    p.add_argument("--channel", default="can0", help="SocketCAN/CAN-FD channel, default can0")
    p.add_argument(
        "--transport",
        default="auto",
        choices=["auto", "socketcan", "socketcanfd", "dm-serial"],
        help="transport backend; dm-serial is Damiao-only",
    )
    p.add_argument("--serial-port", default="/dev/ttyACM0", help="serial port for dm-serial")
    p.add_argument("--serial-baud", type=int, default=921600, help="baud rate for dm-serial")
    p.add_argument("--model", default="4340", help="model name/hint, e.g. 4340P or rs-00")
    p.add_argument("--motor-id", default="0x01", help="command/device ID, hex or decimal")
    p.add_argument(
        "--feedback-id",
        default="0x11",
        help="feedback/host ID, hex or decimal; RobStride commonly uses 0xFD",
    )


def _add_run_args(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--mode",
        default="mit",
        choices=[
            "enable",
            "disable",
            "clear-error",
            "active-report",
            "mit",
            "pos-vel",
            "vel",
            "force-pos",
            "ping",
            "zero",
            "set-zero",
            "save",
            "zero-by-offset",
            "read-param",
            "write-param",
        ],
        help="operation mode; use scan subcommand for wide bus scans",
    )
    p.add_argument("--loop", type=int, default=100, help="send cycles for run mode")
    p.add_argument("--dt-ms", type=int, default=20, help="period between control frames in ms")
    p.add_argument("--ensure-mode", type=int, default=1, help="mode guard before control, 1/0")
    p.add_argument("--ensure-strict", type=int, default=0, help="fail if mode guard cannot confirm, 1/0")
    p.add_argument("--ensure-timeout-ms", type=int, default=1000, help="mode guard timeout")
    p.add_argument("--print-state", type=int, default=1, help="print returned motor state, 1/0")
    p.add_argument("--pos", type=float, default=0.0, help="target position in radians")
    p.add_argument("--vel", type=float, default=0.0, help="target velocity in rad/s")
    p.add_argument("--kp", type=float, default=30.0, help="MIT kp; RobStride pos-vel fallback for loc_kp")
    p.add_argument("--loc-kp", type=float, default=None, help="RobStride pos-vel native position-loop gain")
    p.add_argument("--kd", type=float, default=1.0, help="MIT kd")
    p.add_argument("--tau", type=float, default=0.0, help="target torque/feed-forward torque")
    p.add_argument("--vlim", type=float, default=1.0, help="velocity limit for pos-vel/force-pos")
    p.add_argument("--ratio", type=float, default=0.3, help="force-pos interpolation ratio")
    p.add_argument("--zero-exp", type=int, default=0, help="RobStride experimental zero sequence, 1/0")
    p.add_argument(
        "--offset-negate",
        type=int,
        default=0,
        help="accepted for RobStride zero-by-offset; currently no calibration frames are sent",
    )
    p.add_argument("--store", type=int, default=1, help="save/store after supported write/zero operations, 1/0")
    p.add_argument("--param-id", default="0x0", help="parameter/register ID for read-param/write-param")
    p.add_argument("--param-value", default="", help="value for write-param")
    p.add_argument(
        "--param-type",
        choices=["i8", "u8", "u16", "u32", "f32"],
        default="",
        help="parameter type; inferred for common RobStride params when omitted",
    )
    p.add_argument("--timeout-ms", type=int, default=500, help="operation timeout in ms")
    p.add_argument("--active-report", type=int, default=1, help="RobStride active report on/off for active-report mode, 1/0")
    p.add_argument("--set-motor-id", default="", help="change motor/device ID from run command")
    p.add_argument("--set-feedback-id", default="", help="Damiao feedback ID change from run command")
    p.add_argument("--verify-id", type=int, default=1, help="verify ID change, 1/0")
    p.add_argument("--verify-model", type=int, default=1, help="Damiao model verification before control, 1/0")
    p.add_argument("--verify-timeout-ms", type=int, default=500, help="model/ID verification timeout")
    p.add_argument("--verify-tol", type=float, default=0.2, help="Damiao model limit verification tolerance")


def _vendor_defaults(vendor: str, model: str, feedback_id: str) -> tuple[str, str]:
    resolved_model = model
    resolved_feedback = feedback_id
    if vendor == "robstride":
        if resolved_model == "4340":
            resolved_model = "rs-00"
        if resolved_feedback == "0x11":
            resolved_feedback = "0xFD"
    elif vendor == "myactuator":
        if resolved_model == "4340":
            resolved_model = "X8"
        if resolved_feedback == "0x11":
            resolved_feedback = "0x241"
    elif vendor == "hightorque":
        if resolved_model == "4340":
            resolved_model = "hightorque"
        if resolved_feedback == "0x11":
            resolved_feedback = "0x01"
    elif vendor == "hexfellow":
        if resolved_model == "4340":
            resolved_model = "hexfellow"
        if resolved_feedback == "0x11":
            resolved_feedback = "0x00"
    return resolved_model, resolved_feedback


def _add_motor(ctrl: Controller, vendor: str, motor_id: int, feedback_id: int, model: str):
    if vendor == "myactuator":
        return ctrl.add_myactuator_motor(motor_id, feedback_id, model)
    if vendor == "robstride":
        return ctrl.add_robstride_motor(motor_id, feedback_id, model)
    if vendor == "hightorque":
        return ctrl.add_hightorque_motor(motor_id, feedback_id, model)
    if vendor == "hexfellow":
        return ctrl.add_hexfellow_motor(motor_id, feedback_id, model)
    return ctrl.add_damiao_motor(motor_id, feedback_id, model)


def _open_controller(args: argparse.Namespace, vendor: str) -> Controller:
    transport = getattr(args, "transport", "auto")
    if transport == "dm-serial":
        if vendor != "damiao":
            raise ValueError("transport=dm-serial is supported only for --vendor damiao")
        return Controller.from_dm_serial(args.serial_port, int(args.serial_baud))
    if vendor == "hexfellow":
        if transport == "socketcan":
            raise ValueError("vendor=hexfellow requires --transport socketcanfd (or auto)")
        return Controller.from_socketcanfd(args.channel)
    if transport == "socketcanfd":
        return Controller.from_socketcanfd(args.channel)
    return Controller(args.channel)


def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="motorbridge Python SDK CLI",
        formatter_class=argparse.RawTextHelpFormatter,
        epilog=(
            "Examples:\n"
            "  motorbridge-cli scan --vendor robstride --channel can0 --start-id 1 --end-id 127\n"
            "  motorbridge-cli run --vendor robstride --channel can0 --model rs-00 --motor-id 1 --feedback-id 0xFD --mode enable --loop 1\n"
            "  motorbridge-cli run --vendor robstride --channel can0 --model rs-00 --motor-id 127 --feedback-id 0xFD --mode read-param --param-id 0x7019\n"
            "  motorbridge-cli run --vendor robstride --channel can0 --model rs-00 --motor-id 127 --feedback-id 0xFD --mode pos-vel --pos 0 --vlim 1 --loc-kp 1\n"
            "  motorbridge-cli run --vendor robstride --channel can0 --model rs-00 --motor-id 2 --feedback-id 0xFD --mode clear-error --loop 1\n"
            "  motorbridge-cli run --vendor robstride --channel can0 --model rs-00 --motor-id 2 --feedback-id 0xFD --mode active-report --active-report 1 --loop 1\n"
            "  motorbridge-cli damiao-read-param --channel can0 --model 4340P --motor-id 1 --feedback-id 0x11 --param-id 10 --type f32\n"
        ),
    )
    sub = p.add_subparsers(dest="command")

    run = sub.add_parser("run", help="send control commands (default command)")
    _add_common_args(run)
    _add_run_args(run)

    dump = sub.add_parser("id-dump", help="read key ID/mode/timeout registers")
    _add_common_args(dump)
    dump.add_argument("--timeout-ms", type=int, default=500, help="register read timeout in ms")
    dump.add_argument("--rids", default="7,8,9,10,21,22,23", help="comma-separated Damiao register IDs")

    set_id = sub.add_parser(
        "id-set",
        help="change motor ID; Damiao supports ESC_ID/MST_ID, RobStride supports device_id",
    )
    _add_common_args(set_id)
    set_id.add_argument("--new-motor-id", default="", help="new command/device ID, hex or decimal")
    set_id.add_argument(
        "--new-feedback-id",
        default="",
        help="Damiao MST_ID only; RobStride host_id is not changed",
    )
    set_id.add_argument("--store", type=int, default=1, help="store ID change when supported, 1/0")
    set_id.add_argument("--verify", type=int, default=1, help="verify ID change after write, 1/0")
    set_id.add_argument("--timeout-ms", type=int, default=800, help="verification timeout in ms")

    scan = sub.add_parser("scan", help="scan active motor IDs")
    scan.add_argument(
        "--vendor",
        default="damiao",
        choices=["damiao", "myactuator", "robstride", "hightorque", "hexfellow", "all"],
        help="vendor/protocol to scan, or all for combined scan",
    )
    scan.add_argument("--channel", default="can0", help="SocketCAN/CAN-FD channel")
    scan.add_argument(
        "--transport",
        default="auto",
        choices=["auto", "socketcan", "socketcanfd", "dm-serial"],
        help="transport backend; dm-serial is Damiao-only",
    )
    scan.add_argument("--serial-port", default="/dev/ttyACM0", help="serial port for dm-serial")
    scan.add_argument("--serial-baud", type=int, default=921600, help="baud rate for dm-serial")
    scan.add_argument("--model", default="4340", help="model hint used by vendor scanner")
    scan.add_argument("--start-id", default="0x01", help="first motor/device ID to probe")
    scan.add_argument("--end-id", default="0x10", help="last motor/device ID to probe")
    scan.add_argument(
        "--feedback-ids",
        default="0xFD,0xFF,0xFE,0x00,0xAA",
        help="RobStride host_id candidates; these are not motor/device IDs",
    )
    scan.add_argument("--feedback-base", default="0x10", help="Damiao feedback ID base")
    scan.add_argument("--timeout-ms", type=int, default=80, help="scan ping/status timeout in ms")
    scan.add_argument(
        "--param-id",
        default="0x7019",
        help="RobStride parameter used as scan fallback",
    )
    scan.add_argument(
        "--param-timeout-ms",
        type=int,
        default=120,
        help="RobStride parameter fallback timeout in ms",
    )

    rs_read = sub.add_parser("robstride-read-param", help="read a RobStride parameter")
    _add_common_args(rs_read)
    rs_read.set_defaults(vendor="robstride")
    rs_read.set_defaults(model="rs-00", feedback_id="0xFD")
    rs_read.add_argument("--param-id", required=True, help="RobStride parameter ID, hex or decimal")
    rs_read.add_argument("--type", required=True, choices=["i8", "u8", "u16", "u32", "f32"], help="parameter value type")
    rs_read.add_argument("--timeout-ms", type=int, default=500, help="read timeout in ms")

    rs_write = sub.add_parser("robstride-write-param", help="write a RobStride parameter")
    _add_common_args(rs_write)
    rs_write.set_defaults(vendor="robstride")
    rs_write.set_defaults(model="rs-00", feedback_id="0xFD")
    rs_write.add_argument("--param-id", required=True, help="RobStride parameter ID, hex or decimal")
    rs_write.add_argument("--type", required=True, choices=["i8", "u8", "u16", "u32", "f32"], help="parameter value type")
    rs_write.add_argument("--value", required=True, help="value to write")
    rs_write.add_argument("--verify", type=int, default=1, help="read back after write, 1/0")
    rs_write.add_argument("--timeout-ms", type=int, default=500, help="write/readback timeout in ms")

    dm_read = sub.add_parser("damiao-read-param", help="read a Damiao parameter/register")
    _add_common_args(dm_read)
    dm_read.set_defaults(vendor="damiao")
    dm_read.add_argument("--param-id", required=True, help="Damiao register/parameter ID, hex or decimal")
    dm_read.add_argument("--type", required=True, choices=["u32", "f32"], help="parameter value type")
    dm_read.add_argument("--timeout-ms", type=int, default=500, help="read timeout in ms")

    dm_write = sub.add_parser("damiao-write-param", help="write a Damiao parameter/register")
    _add_common_args(dm_write)
    dm_write.set_defaults(vendor="damiao")
    dm_write.add_argument("--param-id", required=True, help="Damiao register/parameter ID, hex or decimal")
    dm_write.add_argument("--type", required=True, choices=["u32", "f32"], help="parameter value type")
    dm_write.add_argument("--value", required=True, help="value to write")
    dm_write.add_argument("--verify", type=int, default=1, help="read back after write, 1/0")
    dm_write.add_argument("--timeout-ms", type=int, default=500, help="write/readback timeout in ms")

    return p


def _parse_with_legacy_support() -> argparse.Namespace:
    parser = _build_parser()
    if len(sys.argv) > 1 and sys.argv[1].startswith("--") and sys.argv[1] not in ("-h", "--help"):
        legacy = argparse.ArgumentParser(description="motorbridge Python SDK CLI (legacy run mode)")
        _add_common_args(legacy)
        _add_run_args(legacy)
        legacy_args = legacy.parse_args()
        legacy_args.command = "run"
        return legacy_args

    args, extras = parser.parse_known_args()
    if args.command is not None:
        if extras:
            parser.error(f"unrecognized arguments: {' '.join(extras)}")
        return args

    legacy = argparse.ArgumentParser(description="motorbridge Python SDK CLI (legacy run mode)")
    _add_common_args(legacy)
    _add_run_args(legacy)
    legacy_args = legacy.parse_args()
    legacy_args.command = "run"
    return legacy_args


def _run_command(args: argparse.Namespace) -> None:
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    print(
        f"command=run vendor={args.vendor} transport={args.transport} channel={args.channel} "
        f"model={args.model} motor_id=0x{motor_id:X} feedback_id=0x{feedback_id:X} mode={args.mode}"
    )

    if args.set_motor_id or args.set_feedback_id:
        if args.vendor not in ("damiao", "robstride"):
            raise ValueError("run --set-motor-id/--set-feedback-id supports Damiao and RobStride only")
        if args.vendor == "robstride" and args.set_feedback_id:
            raise ValueError("RobStride --set-feedback-id is not supported; feedback_id/host_id is not motor_id")
        id_args = argparse.Namespace(**vars(args))
        id_args.command = "id-set"
        id_args.new_motor_id = args.set_motor_id
        id_args.new_feedback_id = args.set_feedback_id
        id_args.verify = args.verify_id
        id_args.timeout_ms = args.verify_timeout_ms
        _id_set_command(id_args)
        return

    with _open_controller(args, args.vendor) as ctrl:
        motor = _add_motor(ctrl, args.vendor, motor_id, feedback_id, args.model)
        try:
            if args.mode in ("read-param", "write-param"):
                if args.vendor != "robstride":
                    raise ValueError("run --mode read-param/write-param is currently supported for --vendor robstride only")
                param_id = _parse_id(args.param_id)
                param_type = _infer_robstride_param_type(param_id, args.param_type)
                if args.mode == "read-param":
                    value = _read_robstride_param(motor, param_id, param_type, args.timeout_ms)
                    print(
                        f"command=run mode=read-param vendor=robstride param_id=0x{param_id:X} "
                        f"type={param_type} value={value}"
                    )
                else:
                    if args.param_value == "":
                        raise ValueError("run --mode write-param requires --param-value")
                    _write_robstride_param(motor, param_id, param_type, args.param_value)
                    time.sleep(0.05)
                    value = _read_robstride_param(motor, param_id, param_type, args.timeout_ms)
                    print(
                        f"command=run mode=write-param vendor=robstride param_id=0x{param_id:X} "
                        f"type={param_type} value={args.param_value} verify={value}"
                    )
                return
            if args.mode == "save":
                if args.vendor != "robstride":
                    raise ValueError("run --mode save is currently supported for --vendor robstride only")
                motor.store_parameters()
                print("[ok] save-parameters requested")
                return
            if args.mode == "active-report":
                if args.vendor != "robstride":
                    raise ValueError("run --mode active-report is currently supported for --vendor robstride only")
                enabled = bool(args.active_report)
                motor.robstride_set_active_report(enabled)
                print(f"[ok] active report {'enabled' if enabled else 'disabled'}")
                return
            if args.mode == "zero-by-offset":
                if args.vendor != "robstride":
                    raise ValueError("run --mode zero-by-offset is currently supported for --vendor robstride only")
                print(
                    "[warn] robstride zero-by-offset is temporarily disabled due to firmware inconsistency; "
                    "no calibration CAN frames sent"
                )
                return

            if args.vendor == "damiao" and args.verify_model and args.mode not in ("enable", "disable"):
                _verify_damiao_model(motor, args.model, args.verify_timeout_ms, args.verify_tol)

            control_modes = ("mit", "pos-vel", "vel", "force-pos")
            if args.vendor == "robstride" and args.mode in ("mit", "pos-vel", "vel") and args.ensure_mode:
                _ensure_robstride_mode_for_control(
                    ctrl,
                    motor,
                    args.mode,
                    args.ensure_timeout_ms,
                    bool(args.ensure_strict),
                )
                ctrl.enable_all()
                time.sleep(0.1)
            elif args.mode not in ("enable", "disable", "clear-error", "active-report", "ping", "zero", "set-zero"):
                ctrl.enable_all()
                time.sleep(0.3)

            if (
                args.ensure_mode
                and args.mode in control_modes
                and not (args.vendor == "robstride" and args.mode in ("mit", "pos-vel", "vel"))
            ):
                try:
                    if args.vendor == "robstride" and args.mode == "force-pos":
                        raise ValueError("robstride does not support force-pos")
                    motor.ensure_mode(_mode_to_enum(args.mode), args.ensure_timeout_ms)
                except Exception as e:
                    if args.ensure_strict:
                        raise
                    print(f"[warn] ensure_mode failed: {e}; continue anyway")

            for i in range(args.loop):
                if args.mode == "enable":
                    motor.enable()
                    if args.vendor == "damiao":
                        motor.request_feedback()
                elif args.mode == "disable":
                    motor.disable()
                    if args.vendor == "damiao":
                        motor.request_feedback()
                elif args.mode == "clear-error":
                    motor.clear_error()
                    print("[ok] clear-error requested")
                    break
                elif args.mode == "ping":
                    if args.vendor != "robstride":
                        raise ValueError("ping mode is only valid for RobStride")
                    device_id, responder_id = motor.robstride_ping()
                    print(f"#{i} ping device_id={device_id} responder_id={responder_id}")
                    break
                elif args.mode == "mit":
                    if args.vendor == "myactuator":
                        raise ValueError("myactuator does not support mit command")
                    motor.send_mit(args.pos, args.vel, args.kp, args.kd, args.tau)
                elif args.mode == "pos-vel":
                    if args.vendor == "robstride":
                        speed = abs(float(args.vlim))
                        if speed > 0.0:
                            motor.robstride_write_param_f32(0x7017, speed)
                        loc_kp = args.loc_kp if args.loc_kp is not None else args.kp
                        if loc_kp is not None and loc_kp >= 0.0:
                            motor.robstride_write_param_f32(0x701E, float(loc_kp))
                        motor.robstride_write_param_f32(0x7016, float(args.pos))
                    else:
                        motor.send_pos_vel(args.pos, args.vlim)
                elif args.mode == "vel":
                    if args.vendor == "hexfellow":
                        raise ValueError("hexfellow does not support vel command")
                    motor.send_vel(args.vel)
                elif args.mode == "force-pos":
                    if args.vendor in ("robstride", "myactuator", "hexfellow"):
                        raise ValueError(f"{args.vendor} does not support force-pos command")
                    motor.send_force_pos(args.pos, args.vlim, args.ratio)
                elif args.mode in ("zero", "set-zero"):
                    if args.vendor != "robstride":
                        raise ValueError("zero/set-zero mode is currently supported for --vendor robstride only")
                    if not args.zero_exp:
                        print(
                            "[warn] robstride zero requires experimental sequence; "
                            "no CAN frame sent. Re-run with --zero-exp 1"
                        )
                        break
                    # Experimental sequence aligned with core CLI: disable -> set-zero -> optional store.
                    try:
                        motor.disable()
                    except Exception as e:
                        print(f"[warn] pre-zero disable failed: {e}; continue")
                    time.sleep(0.05)
                    motor.set_zero_position()
                    if args.store:
                        motor.store_parameters()
                    print(f"[ok] robstride zero sequence finished (store={int(bool(args.store))})")
                    break

                # Keep feedback state fresh during active control loops.
                if args.vendor == "damiao" and args.mode in ("mit", "pos-vel", "vel", "force-pos"):
                    motor.request_feedback()
                    try:
                        ctrl.poll_feedback_once()
                    except Exception:
                        # Best-effort polling; command loop should keep running.
                        pass

                if args.print_state:
                    st = motor.get_state()
                    if st is None:
                        print(f"#{i} no feedback yet")
                    else:
                        print(
                            f"#{i} pos={st.pos:+.3f} vel={st.vel:+.3f} "
                            f"torq={st.torq:+.3f} status={st.status_code}"
                        )
                time.sleep(max(args.dt_ms, 0) / 1000.0)
        finally:
            motor.close()


def _id_dump_command(args: argparse.Namespace) -> None:
    if args.vendor != "damiao":
        raise ValueError("id-dump currently supports Damiao only")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    rids = _parse_rids(args.rids)
    print(
        f"command=id-dump transport={args.transport} channel={args.channel} model={args.model} "
        f"motor_id=0x{motor_id:X} feedback_id=0x{feedback_id:X}"
    )
    ctrl = _open_controller(args, args.vendor)
    motor = ctrl.add_damiao_motor(motor_id, feedback_id, args.model)
    try:
        for rid in rids:
            try:
                value = motor.get_register_u32(rid, args.timeout_ms)
                print(f"rid={rid:>3} (u32) = {value} (0x{value:X})")
            except Exception as e_u32:
                try:
                    value_f = motor.get_register_f32(rid, args.timeout_ms)
                    print(f"rid={rid:>3} (f32) = {value_f:.6f}")
                except Exception:
                    print(f"rid={rid:>3} read failed: {e_u32}")
    finally:
        motor.close()
        ctrl.close_bus()
        ctrl.close()


def _id_set_command(args: argparse.Namespace) -> None:
    if args.vendor not in ("damiao", "robstride"):
        raise ValueError("id-set currently supports Damiao and RobStride only")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    new_motor_id = _parse_id(args.new_motor_id) if args.new_motor_id else motor_id
    new_feedback_id = _parse_id(args.new_feedback_id) if args.new_feedback_id else feedback_id

    if args.vendor == "robstride":
        _robstride_device_id(motor_id, "motor_id")
        _robstride_device_id(new_motor_id, "new_motor_id")
        _robstride_host_id(feedback_id, "feedback_id")
        if args.new_feedback_id and new_feedback_id != feedback_id:
            raise ValueError(
                "RobStride id-set changes device_id only; feedback_id/host_id is not motor_id"
            )
        print(
            f"command=id-set vendor=robstride transport={args.transport} channel={args.channel} model={args.model} "
            f"old_motor_id=0x{motor_id:X} feedback_id/host_id=0x{feedback_id:X} new_motor_id=0x{new_motor_id:X}"
        )
        print("[info] RobStride feedback_id/host_id is the host-side ID, not the motor/device ID")
        ctrl = _open_controller(args, args.vendor)
        motor = ctrl.add_robstride_motor(motor_id, feedback_id, args.model)
        try:
            motor.robstride_set_device_id(new_motor_id)
            print(f"robstride_set_device_id requested: 0x{motor_id:X} -> 0x{new_motor_id:X}")
            if args.store:
                motor.store_parameters()
                print("save_parameters sent")
        finally:
            motor.close()
            ctrl.close_bus()
            ctrl.close()

        if not args.verify:
            return

        time.sleep(0.12)
        verify_ctrl = _open_controller(args, args.vendor)
        verify_motor = verify_ctrl.add_robstride_motor(new_motor_id, feedback_id, args.model)
        try:
            device_id, responder_id = verify_motor.robstride_ping()
            print(f"verify ping ok: device_id=0x{device_id:X} responder_id=0x{responder_id:X}")
            if device_id != new_motor_id:
                raise RuntimeError(
                    f"verify failed: expected device_id=0x{new_motor_id:X}, got 0x{device_id:X}"
                )
            print("verify ok")
        finally:
            verify_motor.close()
            verify_ctrl.close_bus()
            verify_ctrl.close()
        return

    print(
        f"command=id-set vendor=damiao transport={args.transport} channel={args.channel} model={args.model} "
        f"old_motor_id=0x{motor_id:X} old_feedback_id=0x{feedback_id:X} "
        f"new_motor_id=0x{new_motor_id:X} new_feedback_id=0x{new_feedback_id:X}"
    )

    ctrl = _open_controller(args, args.vendor)
    motor = ctrl.add_damiao_motor(motor_id, feedback_id, args.model)
    try:
        if new_feedback_id != feedback_id:
            motor.write_register_u32(7, new_feedback_id)
            print(f"write rid=7 (MST_ID) <= 0x{new_feedback_id:X}")
        if new_motor_id != motor_id:
            motor.write_register_u32(8, new_motor_id)
            print(f"write rid=8 (ESC_ID) <= 0x{new_motor_id:X}")
        if args.store:
            motor.store_parameters()
            print("store_parameters sent")
    finally:
        motor.close()
        ctrl.close_bus()
        ctrl.close()

    if not args.verify:
        return

    verify_ctrl = _open_controller(args, args.vendor)
    verify_motor = verify_ctrl.add_damiao_motor(new_motor_id, new_feedback_id, args.model)
    try:
        esc = verify_motor.get_register_u32(8, args.timeout_ms)
        mst = verify_motor.get_register_u32(7, args.timeout_ms)
        print(f"verify rid=8 (ESC_ID): 0x{esc:X}")
        print(f"verify rid=7 (MST_ID): 0x{mst:X}")
        if esc != new_motor_id or mst != new_feedback_id:
            raise RuntimeError(
                f"verify failed: expected ESC_ID=0x{new_motor_id:X}, MST_ID=0x{new_feedback_id:X}, "
                f"got ESC_ID=0x{esc:X}, MST_ID=0x{mst:X}"
            )
        print("verify ok")
    finally:
        verify_motor.close()
        verify_ctrl.close_bus()
        verify_ctrl.close()


def _scan_damiao(args: argparse.Namespace, start_id: int, end_id: int) -> list[tuple[int, str]]:
    feedback_base = _parse_id(args.feedback_base)
    found: list[tuple[int, str]] = []
    print(
        f"[scan:damiao] channel={args.channel} model={args.model} "
        f"id_range=[0x{start_id:X},0x{end_id:X}] timeout_ms={args.timeout_ms}"
    )
    ctrl = _open_controller(args, "damiao")
    try:
        for mid in range(start_id, end_id + 1):
            fid = feedback_base + (mid & 0x0F)
            motor = ctrl.add_damiao_motor(mid, fid, args.model)
            try:
                esc_id = motor.get_register_u32(8, args.timeout_ms)
                mst_id = motor.get_register_u32(7, args.timeout_ms)
                found.append((mid, f"vendor=damiao esc_id=0x{esc_id:X} mst_id=0x{mst_id:X}"))
                print(f"[hit] vendor=damiao probe=0x{mid:02X} esc_id=0x{esc_id:X} mst_id=0x{mst_id:X}")
            except Exception:
                print(f"[.. ] vendor=damiao probe=0x{mid:02X} no reply")
            finally:
                motor.close()
    finally:
        ctrl.close_bus()
        ctrl.close()
    return found


def _scan_robstride(args: argparse.Namespace, start_id: int, end_id: int) -> list[tuple[int, str]]:
    feedback_ids = _parse_rids(args.feedback_ids)
    if not 1 <= start_id <= 255 or not 1 <= end_id <= 255:
        raise ValueError("RobStride scan range must be within 1..255")
    for fid in feedback_ids:
        _robstride_host_id(fid, "feedback-ids")
    param_id = _parse_id(args.param_id)
    found: list[tuple[int, str]] = []
    print(
        f"[scan:robstride] channel={args.channel} model={args.model} "
        f"id_range=[0x{start_id:X},0x{end_id:X}] timeout_ms={args.timeout_ms} "
        f"feedback_ids={','.join(f'0x{x:X}' for x in feedback_ids)} param_id=0x{param_id:X}"
    )
    print(
        "[scan:robstride] note: probe/device_id is the motor ID; "
        "feedback_id/host_id is the host-side ID"
    )
    for mid in range(start_id, end_id + 1):
        hit_meta = None
        for fid in feedback_ids:
            ctrl = Controller(args.channel)
            try:
                motor = ctrl.add_robstride_motor(mid, fid, args.model)
                try:
                    try:
                        device_id, responder_id = motor.robstride_ping_host_id(fid, args.timeout_ms)
                        hit_meta = (
                            f"vendor=robstride via=ping feedback_id=0x{fid:X} "
                            f"device_id={device_id} responder_id={responder_id}"
                        )
                        break
                    except Exception:
                        try:
                            value = motor.robstride_get_param_f32_host_id(
                                param_id, fid, args.param_timeout_ms
                            )
                            hit_meta = (
                                f"vendor=robstride via=read-param feedback_id=0x{fid:X} "
                                f"param_id=0x{param_id:X} value={value}"
                            )
                            break
                        except Exception:
                            # Keep probing next feedback candidate / next ID on timeout/no-response.
                            pass
                finally:
                    motor.close()
            finally:
                ctrl.close_bus()
                ctrl.close()
        if hit_meta is None:
            print(f"[.. ] vendor=robstride probe=0x{mid:02X} no reply")
        else:
            found.append((mid, hit_meta))
            print(f"[hit] probe=0x{mid:02X} {hit_meta}")
    return found


def _scan_myactuator(args: argparse.Namespace, start_id: int, end_id: int) -> list[tuple[int, str]]:
    found: list[tuple[int, str]] = []
    lo = max(1, start_id)
    hi = min(32, end_id)
    print(
        f"[scan:myactuator] channel={args.channel} model={args.model} "
        f"id_range=[0x{lo:X},0x{hi:X}] timeout_ms={args.timeout_ms}"
    )
    ctrl = Controller(args.channel)
    try:
        for mid in range(lo, hi + 1):
            fid = 0x240 + mid
            motor = ctrl.add_myactuator_motor(mid, fid, args.model)
            try:
                try:
                    motor.request_feedback()
                    time.sleep(min(max(args.timeout_ms, 10), 300) / 1000.0)
                    ctrl.poll_feedback_once()
                    st = motor.get_state()
                    if st is None:
                        raise RuntimeError("no feedback")
                    meta = (
                        f"vendor=myactuator feedback_id=0x{fid:X} "
                        f"temp={st.t_mos:.1f}C vel={st.vel:+.3f}rad/s angle={st.pos:+.3f}rad"
                    )
                    found.append((mid, meta))
                    print(f"[hit] probe=0x{mid:02X} {meta}")
                except Exception:
                    print(f"[.. ] vendor=myactuator probe=0x{mid:02X} no reply")
            finally:
                motor.close()
    finally:
        ctrl.close_bus()
        ctrl.close()
    return found


def _scan_hightorque(args: argparse.Namespace, start_id: int, end_id: int) -> list[tuple[int, str]]:
    found: list[tuple[int, str]] = []
    lo = max(1, start_id)
    hi = min(127, end_id)
    print(
        f"[scan:hightorque] channel={args.channel} model={args.model} "
        f"id_range=[0x{lo:X},0x{hi:X}] timeout_ms={args.timeout_ms}"
    )
    ctrl = Controller(args.channel)
    try:
        for mid in range(lo, hi + 1):
            motor = ctrl.add_hightorque_motor(mid, 0x01, args.model)
            try:
                try:
                    motor.request_feedback()
                    st = motor.get_state()
                    if st is None:
                        raise RuntimeError("no feedback")
                    meta = (
                        f"vendor=hightorque pos={st.pos:+.4f}rad "
                        f"vel={st.vel:+.4f}rad/s torq={st.torq:+.3f}Nm"
                    )
                    found.append((mid, meta))
                    print(f"[hit] probe=0x{mid:02X} {meta}")
                except Exception:
                    print(f"[.. ] vendor=hightorque probe=0x{mid:02X} no reply")
            finally:
                motor.close()
    finally:
        ctrl.close_bus()
        ctrl.close()
    return found


def _scan_command(args: argparse.Namespace) -> None:
    start_id = _parse_id(args.start_id)
    end_id = _parse_id(args.end_id)
    if end_id < start_id:
        raise ValueError("end-id must be >= start-id")
    if args.transport == "dm-serial" and args.vendor != "damiao":
        raise ValueError("scan with transport=dm-serial currently supports --vendor damiao only")
    resolved_model, _ = _vendor_defaults(args.vendor if args.vendor != "all" else "damiao", args.model, "0x11")
    args.model = resolved_model
    print(
        f"command=scan vendor={args.vendor} transport={args.transport} channel={args.channel} model={args.model} "
        f"id_range=[0x{start_id:X},0x{end_id:X}] timeout_ms={args.timeout_ms}"
    )

    found: list[tuple[int, str]] = []
    damiao_model = args.model
    myactuator_model = "X8" if args.model == "4340" else args.model
    robstride_model = "rs-00" if args.model == "4340" else args.model
    hightorque_model = "hightorque" if args.model == "4340" else args.model
    if args.vendor in ("damiao", "all"):
        args.model = damiao_model
        found.extend(_scan_damiao(args, start_id, end_id))
    if args.vendor in ("myactuator", "all"):
        args.model = myactuator_model
        found.extend(_scan_myactuator(args, start_id, end_id))
    if args.vendor in ("robstride", "all"):
        args.model = robstride_model
        found.extend(_scan_robstride(args, start_id, end_id))
    if args.vendor in ("hightorque", "all"):
        args.model = hightorque_model
        found.extend(_scan_hightorque(args, start_id, end_id))

    print(f"scan done: {len(found)} motor(s) found")
    for probe, meta in found:
        print(f"  probe=0x{probe:02X} {meta}")


def _robstride_read_param_command(args: argparse.Namespace) -> None:
    if args.vendor != "robstride":
        raise ValueError("robstride-read-param is only valid for --vendor robstride")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    param_id = _parse_id(args.param_id)
    with _open_controller(args, args.vendor) as ctrl:
        motor = ctrl.add_robstride_motor(motor_id, feedback_id, args.model)
        try:
            value = _read_robstride_param(motor, param_id, args.type, args.timeout_ms)
            print(
                f"command=robstride-read-param channel={args.channel} model={args.model} "
                f"motor_id=0x{motor_id:X} param_id=0x{param_id:X} type={args.type} value={value}"
            )
        finally:
            motor.close()


def _robstride_write_param_command(args: argparse.Namespace) -> None:
    if args.vendor != "robstride":
        raise ValueError("robstride-write-param is only valid for --vendor robstride")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    param_id = _parse_id(args.param_id)
    with _open_controller(args, args.vendor) as ctrl:
        motor = ctrl.add_robstride_motor(motor_id, feedback_id, args.model)
        try:
            _write_robstride_param(motor, param_id, args.type, args.value)
            verify = _read_robstride_param(motor, param_id, args.type, args.timeout_ms) if args.verify else None
            print(
                f"command=robstride-write-param channel={args.channel} model={args.model} "
                f"motor_id=0x{motor_id:X} param_id=0x{param_id:X} type={args.type} "
                f"value={args.value} verify={verify}"
            )
        finally:
            motor.close()


def _damiao_read_param_command(args: argparse.Namespace) -> None:
    if args.vendor != "damiao":
        raise ValueError("damiao-read-param is only valid for --vendor damiao")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    param_id = _parse_id(args.param_id)
    with _open_controller(args, args.vendor) as ctrl:
        motor = ctrl.add_damiao_motor(motor_id, feedback_id, args.model)
        try:
            if args.type == "u32":
                value = motor.damiao_get_param_u32(param_id, args.timeout_ms)
            else:
                value = motor.damiao_get_param_f32(param_id, args.timeout_ms)
            print(
                f"command=damiao-read-param transport={args.transport} channel={args.channel} model={args.model} "
                f"motor_id=0x{motor_id:X} feedback_id=0x{feedback_id:X} "
                f"param_id=0x{param_id:X} type={args.type} value={value}"
            )
        finally:
            motor.close()


def _damiao_write_param_command(args: argparse.Namespace) -> None:
    if args.vendor != "damiao":
        raise ValueError("damiao-write-param is only valid for --vendor damiao")
    args.model, args.feedback_id = _vendor_defaults(args.vendor, args.model, args.feedback_id)
    motor_id = _parse_id(args.motor_id)
    feedback_id = _parse_id(args.feedback_id)
    param_id = _parse_id(args.param_id)
    with _open_controller(args, args.vendor) as ctrl:
        motor = ctrl.add_damiao_motor(motor_id, feedback_id, args.model)
        try:
            if args.type == "u32":
                motor.damiao_write_param_u32(param_id, int(args.value, 0))
                verify = motor.damiao_get_param_u32(param_id, args.timeout_ms) if args.verify else None
            else:
                motor.damiao_write_param_f32(param_id, float(args.value))
                verify = motor.damiao_get_param_f32(param_id, args.timeout_ms) if args.verify else None
            print(
                f"command=damiao-write-param transport={args.transport} channel={args.channel} model={args.model} "
                f"motor_id=0x{motor_id:X} feedback_id=0x{feedback_id:X} "
                f"param_id=0x{param_id:X} type={args.type} value={args.value} verify={verify}"
            )
        finally:
            motor.close()


def main() -> None:
    args = _parse_with_legacy_support()
    try:
        transport = str(getattr(args, "transport", "auto") or "auto")
        channel = str(getattr(args, "channel", "can0") or "can0")
        hint = preflight_can_runtime("motorbridge-cli", transport, channel)
        if hint:
            raise RuntimeError(hint)

        if args.command == "run":
            _run_command(args)
        elif args.command == "id-dump":
            _id_dump_command(args)
        elif args.command == "id-set":
            _id_set_command(args)
        elif args.command == "scan":
            _scan_command(args)
        elif args.command == "robstride-read-param":
            _robstride_read_param_command(args)
        elif args.command == "robstride-write-param":
            _robstride_write_param_command(args)
        elif args.command == "damiao-read-param":
            _damiao_read_param_command(args)
        elif args.command == "damiao-write-param":
            _damiao_write_param_command(args)
        else:
            raise RuntimeError(f"unknown command: {args.command}")
    except Exception as e:
        print(f"[motorbridge-cli] {e}", file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    main()
