use crate::bus::{CanBus, CanFrame};
use crate::error::{MotorError, Result};
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const USB2CANFD: c_int = 0;
const USB2CANFD_DUAL: c_int = 1;
const LINKX4C: c_int = 2;
const CAN_EFF_MASK: u32 = 0x1FFF_FFFF;
const CAN_SFF_MASK: u32 = 0x0000_07FF;

#[repr(C)]
struct MbDmFrame {
    can_id: u32,
    data: [u8; 8],
    dlc: u8,
    channel: u8,
    ext: u8,
    canfd: u8,
}

unsafe extern "C" {
    fn mb_dm_open(
        library_path: *const c_char,
        device_type: c_int,
        selected_channel: u8,
        can_baudrate: u32,
        canfd_baudrate: u32,
        out: *mut *mut c_void,
        err_buf: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn mb_dm_send(
        handle: *mut c_void,
        can_id: u32,
        ext: u8,
        dlc: u8,
        data: *const u8,
        err_buf: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn mb_dm_recv(
        handle: *mut c_void,
        out: *mut MbDmFrame,
        timeout_ms: u32,
        err_buf: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn mb_dm_shutdown(handle: *mut c_void);
}

#[derive(Debug, Clone, Copy)]
pub enum DmDeviceType {
    Usb2CanFd,
    Usb2CanFdDual,
    LinkX4C,
}

impl DmDeviceType {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "usb2canfd" => Ok(Self::Usb2CanFd),
            "usb2canfd-dual" | "usb2canfd_dual" | "dual" => Ok(Self::Usb2CanFdDual),
            "linkx4c" => Ok(Self::LinkX4C),
            _ => Err(MotorError::InvalidArgument(format!(
                "unknown --dm-device-type {raw}, expected usb2canfd|usb2canfd-dual|linkx4c"
            ))),
        }
    }

    fn sdk_value(self) -> c_int {
        match self {
            Self::Usb2CanFd => USB2CANFD,
            Self::Usb2CanFdDual => USB2CANFD_DUAL,
            Self::LinkX4C => LINKX4C,
        }
    }
}

pub fn parse_dm_channel(raw: &str) -> Result<u8> {
    match raw.to_ascii_lowercase().as_str() {
        "0" | "1" | "canfd1" | "can1" | "ch0" | "channel0" => Ok(0),
        "2" | "canfd2" | "can2" | "ch1" | "channel1" => Ok(1),
        _ => Err(MotorError::InvalidArgument(format!(
            "unknown --dm-channel {raw}, expected canfd1|canfd2"
        ))),
    }
}

pub struct DmDeviceBus {
    handle: Mutex<usize>,
    channel: u8,
}

impl DmDeviceBus {
    pub fn open(device_type: DmDeviceType, dm_channel: &str) -> Result<Self> {
        Self::open_with_config(device_type, dm_channel, 1_000_000, 5_000_000)
    }

    pub fn open_with_config(
        device_type: DmDeviceType,
        dm_channel: &str,
        can_baudrate: u32,
        canfd_baudrate: u32,
    ) -> Result<Self> {
        let channel = parse_dm_channel(dm_channel)?;
        if matches!(device_type, DmDeviceType::Usb2CanFd) && channel != 0 {
            return Err(MotorError::InvalidArgument(
                "usb2canfd has one physical channel; use --dm-channel canfd1".to_string(),
            ));
        }

        let library_path = CString::new(resolve_library_path()?.to_string_lossy().as_bytes())
            .map_err(|_| MotorError::InvalidArgument("DM_Device SDK path contains NUL".into()))?;
        let mut err = ErrorBuf::new();
        let mut raw: *mut c_void = std::ptr::null_mut();
        let rc = unsafe {
            mb_dm_open(
                library_path.as_ptr(),
                device_type.sdk_value(),
                channel,
                can_baudrate,
                canfd_baudrate,
                &mut raw as *mut *mut c_void,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if rc != 0 || raw.is_null() {
            return Err(MotorError::Io(err.message_or("mb_dm_open failed")));
        }

        Ok(Self {
            handle: Mutex::new(raw as usize),
            channel,
        })
    }
}

impl CanBus for DmDeviceBus {
    fn send(&self, frame: CanFrame) -> Result<()> {
        if frame.dlc > 8 {
            return Err(MotorError::InvalidArgument(format!(
                "invalid DLC {}, expected <= 8",
                frame.dlc
            )));
        }
        if !frame.is_extended && frame.arbitration_id > CAN_SFF_MASK {
            return Err(MotorError::InvalidArgument(format!(
                "invalid arbitration_id {:X}, expected 11-bit std id",
                frame.arbitration_id
            )));
        }
        if frame.is_extended && frame.arbitration_id > CAN_EFF_MASK {
            return Err(MotorError::InvalidArgument(format!(
                "invalid arbitration_id {:X}, expected 29-bit ext id",
                frame.arbitration_id
            )));
        }

        if debug_enabled() {
            eprintln!(
                "[DM-DEVICE TX] CANFD{} sdk_ch={} CAN {} can_id=0x{:X} dlc={} data=[{}]",
                self.channel + 1,
                self.channel,
                if frame.is_extended { "EXT" } else { "STD" },
                frame.arbitration_id,
                frame.dlc,
                format_payload(&frame.data[..frame.dlc as usize])
            );
        }

        let handle = self.handle_ptr()?;
        let mut err = ErrorBuf::new();
        let rc = unsafe {
            mb_dm_send(
                handle,
                frame.arbitration_id,
                u8::from(frame.is_extended),
                frame.dlc,
                frame.data.as_ptr(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if rc != 0 {
            return Err(MotorError::Io(err.message_or("mb_dm_send failed")));
        }
        Ok(())
    }

    fn recv(&self, timeout: Duration) -> Result<Option<CanFrame>> {
        let handle = self.handle_ptr()?;
        let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
        let mut frame = MbDmFrame {
            can_id: 0,
            data: [0; 8],
            dlc: 0,
            channel: 0,
            ext: 0,
            canfd: 0,
        };
        let mut err = ErrorBuf::new();
        let rc = unsafe {
            mb_dm_recv(
                handle,
                &mut frame as *mut MbDmFrame,
                timeout_ms,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if rc < 0 {
            return Err(MotorError::Io(err.message_or("mb_dm_recv failed")));
        }
        if rc == 0 {
            return Ok(None);
        }

        let out = CanFrame {
            arbitration_id: frame.can_id,
            data: frame.data,
            dlc: frame.dlc.min(8),
            is_extended: frame.ext != 0,
            is_rx: true,
        };
        if debug_enabled() {
            eprintln!(
                "[DM-DEVICE RX] CANFD{} sdk_ch={} CAN {} can_id=0x{:X} dlc={} data=[{}]",
                frame.channel + 1,
                frame.channel,
                if out.is_extended { "EXT" } else { "STD" },
                out.arbitration_id,
                out.dlc,
                format_payload(&out.data[..out.dlc as usize])
            );
        }
        Ok(Some(out))
    }

    fn shutdown(&self) -> Result<()> {
        let mut guard = self
            .handle
            .lock()
            .map_err(|_| MotorError::Io("dm-device handle lock poisoned".to_string()))?;
        if *guard != 0 {
            unsafe { mb_dm_shutdown(*guard as *mut c_void) };
            *guard = 0;
        }
        Ok(())
    }
}

impl DmDeviceBus {
    fn handle_ptr(&self) -> Result<*mut c_void> {
        let handle = *self
            .handle
            .lock()
            .map_err(|_| MotorError::Io("dm-device handle lock poisoned".to_string()))?;
        if handle == 0 {
            return Err(MotorError::Io(
                "dm-device handle already closed".to_string(),
            ));
        }
        Ok(handle as *mut c_void)
    }
}

impl Drop for DmDeviceBus {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.handle.lock() {
            if *guard != 0 {
                unsafe { mb_dm_shutdown(*guard as *mut c_void) };
                *guard = 0;
            }
        }
    }
}

fn resolve_library_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("MOTOR_DM_DEVICE_LIB") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(MotorError::Io(format!(
            "MOTOR_DM_DEVICE_LIB points to missing file: {:?}",
            path
        )));
    }

    let rel = platform_library_relative_path()?;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf);
    let mut candidates = Vec::new();
    if let Some(root) = repo_root {
        candidates.push(root.join("third_party/dm_device/v1.1.0").join(rel));
        candidates.push(root.join("dm-device-sdk/C&C++/lib/v1.1.0").join(rel));
        candidates.push(root.join("../dm-device-sdk/C&C++/lib/v1.1.0").join(rel));
    }
    candidates.push(PathBuf::from("third_party/dm_device/v1.1.0").join(rel));
    candidates.push(PathBuf::from("dm-device-sdk/C&C++/lib/v1.1.0").join(rel));
    candidates.push(PathBuf::from("../dm-device-sdk/C&C++/lib/v1.1.0").join(rel));
    candidates.push(PathBuf::from("lib/dm_device").join(dm_device_library_name()));
    candidates.push(PathBuf::from("motorbridge/lib/dm_device").join(dm_device_library_name()));

    for path in candidates {
        if path.exists() {
            return Ok(path);
        }
    }
    Ok(PathBuf::from(dm_device_library_name()))
}

fn platform_library_relative_path() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux/x86_64/libdm_device.so"),
        ("linux", "aarch64") => Ok("linux/arm64/libdm_device.so"),
        ("macos", "aarch64") => Ok("macos/arm64/libdm_device.dylib"),
        ("macos", "x86_64") => Ok("macos/x86_64/libdm_device.dylib"),
        ("windows", _) => Ok("windows/msvc/dm_device.dll"),
        (os, arch) => Err(MotorError::Unsupported(format!(
            "DM_Device SDK is not bundled for platform {os}/{arch}"
        ))),
    }
}

fn dm_device_library_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "dm_device.dll",
        "macos" => "libdm_device.dylib",
        _ => "libdm_device.so",
    }
}

fn debug_enabled() -> bool {
    std::env::var("MOTOR_DM_DEVICE_DEBUG")
        .ok()
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false)
}

fn format_payload(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

struct ErrorBuf {
    buf: [c_char; 512],
}

impl ErrorBuf {
    fn new() -> Self {
        Self { buf: [0; 512] }
    }

    fn as_mut_ptr(&mut self) -> *mut c_char {
        self.buf.as_mut_ptr()
    }

    fn len(&self) -> usize {
        self.buf.len()
    }

    fn message_or(&self, fallback: &str) -> String {
        let bytes = self
            .buf
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as u8)
            .collect::<Vec<_>>();
        if bytes.is_empty() {
            fallback.to_string()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }
}
