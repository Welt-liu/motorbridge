use crate::protocol::{
    build_ext_id, decode_fault_report, decode_ping_reply, decode_read_parameter_value,
    decode_status_frame, encode_mit_command, encode_parameter_read, encode_parameter_value,
    encode_parameter_write, ext_id_parts, CommunicationType, FaultReport, PingReply,
};
use crate::registers::{parameter_info, ParameterDataType, ParameterId};
use motor_core::bus::{CanBus, CanFrame};
use motor_core::device::MotorDevice;
use motor_core::error::{MotorError, Result};
use motor_core::model::{ModelCatalog, MotorModelSpec, PvTLimits, StaticModelCatalog};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ROBSTRIDE_MODELS: &[MotorModelSpec] = &[
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-00",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 50.0,
        tmax: 17.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-01",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 44.0,
        tmax: 17.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-02",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 44.0,
        tmax: 17.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-03",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 50.0,
        tmax: 60.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-04",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 15.0,
        tmax: 120.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-05",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 33.0,
        tmax: 17.0,
    },
    MotorModelSpec {
        vendor: "robstride",
        model: "rs-06",
        pmax: 4.0 * std::f32::consts::PI,
        vmax: 20.0,
        tmax: 60.0,
    },
];

const ROBSTRIDE_CATALOG: StaticModelCatalog = StaticModelCatalog {
    vendor_name: "robstride",
    models: ROBSTRIDE_MODELS,
};

pub fn model_limits(model: &str) -> Option<(f32, f32, f32)> {
    ROBSTRIDE_CATALOG
        .get(model)
        .map(|spec| (spec.pmax, spec.vmax, spec.tmax))
}

#[derive(Debug, Clone, Copy)]
pub enum ControlMode {
    Mit = 0,
    Position = 1,
    Velocity = 2,
}

#[derive(Debug, Clone, Copy)]
pub enum ParameterValue {
    I8(i8),
    U8(u8),
    U16(u16),
    U32(u32),
    F32(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct MotorFeedbackState {
    pub arbitration_id: u32,
    pub device_id: u8,
    pub position: f32,
    pub velocity: f32,
    pub torque: f32,
    pub temperature_c: f32,
    pub uncalibrated: bool,
    pub stall: bool,
    pub magnetic_encoder_fault: bool,
    pub overtemperature: bool,
    pub overcurrent: bool,
    pub undervoltage: bool,
}

pub struct RobstrideMotor {
    pub motor_id: u16,
    pub feedback_id: u16,
    pub model: String,
    bus: Arc<dyn CanBus>,
    limits: PvTLimits,
    kp_max: f32,
    kd_max: f32,
    state: Mutex<Option<MotorFeedbackState>>,
    fault_report: Mutex<Option<FaultReport>>,
    status_seq: AtomicU64,
    param_state: Mutex<ParameterState>,
    ping_reply: Mutex<Option<PingReply>>,
}

#[derive(Default)]
struct ParameterState {
    values: HashMap<u16, ParameterValue>,
    pending: Option<u16>,
}

impl RobstrideMotor {
    pub fn new(motor_id: u16, feedback_id: u16, model: &str, bus: Arc<dyn CanBus>) -> Result<Self> {
        Self::validate_device_id(motor_id, "motor_id")?;
        Self::validate_host_id(feedback_id, "feedback_id")?;
        let spec = ROBSTRIDE_CATALOG.get(model).ok_or_else(|| {
            MotorError::InvalidArgument(format!("unknown RobStride model: {model}"))
        })?;
        let (kp_max, kd_max) = match model {
            "rs-00" | "rs-01" | "rs-02" | "rs-05" => (500.0, 5.0),
            "rs-03" | "rs-04" | "rs-06" => (5000.0, 100.0),
            _ => (500.0, 5.0),
        };
        Ok(Self {
            motor_id,
            feedback_id,
            model: model.to_string(),
            bus,
            limits: PvTLimits::from_spec(spec),
            kp_max,
            kd_max,
            state: Mutex::new(None),
            fault_report: Mutex::new(None),
            status_seq: AtomicU64::new(0),
            param_state: Mutex::new(ParameterState::default()),
            ping_reply: Mutex::new(None),
        })
    }

    fn device_id_u8(&self) -> Result<u8> {
        Ok(self.motor_id as u8)
    }

    fn host_id_u16(&self) -> u16 {
        self.feedback_id
    }

    fn host_id_u8(&self) -> u8 {
        self.host_id_u16() as u8
    }

    fn host_id_candidates(&self) -> Vec<u16> {
        let mut cands = Vec::new();
        cands.push(self.feedback_id);
        cands.push(0x00FD);
        cands.push(0x00FF);
        cands.push(0x00FE);
        cands.dedup();
        cands
    }

    fn validate_device_id(id: u16, name: &str) -> Result<()> {
        if (1..=255).contains(&id) {
            Ok(())
        } else {
            Err(MotorError::InvalidArgument(format!(
                "RobStride {name} must be in 1..255, got {id}"
            )))
        }
    }

    fn validate_host_id(id: u16, name: &str) -> Result<()> {
        if id <= 255 {
            Ok(())
        } else {
            Err(MotorError::InvalidArgument(format!(
                "RobStride {name}/host_id must be in 0..255, got {id}"
            )))
        }
    }

    fn send_ext(&self, comm_type: u32, extra_data: u16, data: [u8; 8], dlc: u8) -> Result<()> {
        self.bus.send(CanFrame {
            arbitration_id: build_ext_id(comm_type, extra_data, self.device_id_u8()?),
            data,
            dlc,
            is_extended: true,
            is_rx: false,
        })
    }

    fn control_mode_value(mode: ControlMode) -> i8 {
        mode as i8
    }

    fn send_with_status_ack(
        &self,
        comm_type: u32,
        data: [u8; 8],
        dlc: u8,
        timeout: Duration,
    ) -> Result<()> {
        let cands = self.host_id_candidates();
        let per_try = Duration::from_millis((timeout.as_millis() as u64).max(120));
        for host in cands {
            let start_seq = self.status_seq.load(Ordering::Acquire);
            self.send_ext(comm_type, host, data, dlc)?;
            let deadline = Instant::now() + per_try;
            while Instant::now() < deadline {
                if self.status_seq.load(Ordering::Acquire) > start_seq {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(4));
            }
        }
        Err(MotorError::Timeout(format!(
            "control ack timeout: comm_type={comm_type}"
        )))
    }

    pub fn ping(&self, timeout: Duration) -> Result<PingReply> {
        let cands = self.host_id_candidates();
        let per_try = Duration::from_millis((timeout.as_millis() as u64).max(120));
        for host in cands {
            if let Ok(reply) = self.ping_with_host_id(host, per_try) {
                return Ok(reply);
            }
        }
        Err(MotorError::Timeout(format!(
            "ping {} timed out",
            self.motor_id
        )))
    }

    pub fn ping_with_host_id(&self, host_id: u16, timeout: Duration) -> Result<PingReply> {
        Self::validate_host_id(host_id, "feedback_id")?;
        self.ping_reply
            .lock()
            .map_err(|_| MotorError::Io("ping reply lock poisoned".to_string()))?
            .take();
        self.send_ext(CommunicationType::GET_DEVICE_ID, host_id, [0u8; 8], 8)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(reply) = *self
                .ping_reply
                .lock()
                .map_err(|_| MotorError::Io("ping reply lock poisoned".to_string()))?
            {
                return Ok(reply);
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        Err(MotorError::Timeout(format!(
            "ping {} timed out for host_id 0x{host_id:X}",
            self.motor_id
        )))
    }

    pub fn set_mode(&self, mode: ControlMode) -> Result<()> {
        self.write_parameter(ParameterId::Mode as u16, ParameterValue::I8(mode as i8))
    }

    pub fn ensure_control_mode(&self, mode: ControlMode, timeout: Duration) -> Result<()> {
        let desired = Self::control_mode_value(mode);
        let read_timeout = timeout.max(Duration::from_millis(150));

        if let Ok(current) = self.get_parameter_i8(ParameterId::Mode as u16, read_timeout) {
            if current == desired {
                return Ok(());
            }
        }

        // Some RobStride firmware variants ignore run_mode writes while torque is
        // enabled. Disable first, but still rely on the final run_mode readback
        // as the source of truth.
        let mut last_error = self.disable().err().map(|e| e.to_string());
        std::thread::sleep(Duration::from_millis(60));

        let mut actual = None;
        for _ in 0..3 {
            if let Err(err) = self.set_mode(mode) {
                last_error = Some(err.to_string());
                std::thread::sleep(Duration::from_millis(30));
                continue;
            }

            std::thread::sleep(Duration::from_millis(30));
            match self.get_parameter_i8(ParameterId::Mode as u16, read_timeout) {
                Ok(value) if value == desired => return Ok(()),
                Ok(value) => {
                    actual = Some(value);
                    last_error = None;
                }
                Err(err) => {
                    last_error = Some(err.to_string());
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }

        Err(MotorError::Protocol(format!(
            "control mode verify failed: expected {desired}, got {actual:?}, last_error={last_error:?}"
        )))
    }

    pub fn set_zero_position(&self) -> Result<()> {
        let mut payload = [0u8; 8];
        payload[0] = 0x01;
        self.send_with_status_ack(
            CommunicationType::SET_ZERO_POSITION,
            payload,
            1,
            Duration::from_millis(320),
        )?;

        // RobStride zeroing is two-part: type 6 sets the mechanical zero, while
        // zero_sta selects the startup coordinate range. Keep the unified upper
        // set-zero API and make RobStride default to -pi..pi after zeroing.
        self.write_parameter(ParameterId::ZeroState as u16, ParameterValue::U8(1))
    }

    pub fn save_parameters(&self) -> Result<()> {
        self.send_ext(
            CommunicationType::SAVE_PARAMETERS,
            self.host_id_u16(),
            [1, 2, 3, 4, 5, 6, 7, 8],
            8,
        )
    }

    pub fn set_device_id(&self, new_id: u8) -> Result<()> {
        Self::validate_device_id(u16::from(new_id), "new_device_id")?;
        let extra = (u16::from(new_id) << 8) | u16::from(self.host_id_u8());
        let payload = self.ping(Duration::from_millis(140))?.payload;
        self.send_ext(CommunicationType::SET_DEVICE_ID, extra, payload, 8)
    }

    pub fn enable(&self) -> Result<()> {
        self.send_with_status_ack(
            CommunicationType::ENABLE,
            [0u8; 8],
            8,
            Duration::from_millis(240),
        )
    }

    pub fn disable(&self) -> Result<()> {
        self.send_with_status_ack(
            CommunicationType::DISABLE,
            [0u8; 8],
            8,
            Duration::from_millis(240),
        )
    }

    pub fn clear_error(&self) -> Result<()> {
        self.send_with_status_ack(
            CommunicationType::DISABLE,
            [1, 0, 0, 0, 0, 0, 0, 0],
            8,
            Duration::from_millis(240),
        )
    }

    pub fn set_active_report(&self, enabled: bool) -> Result<()> {
        let cmd = if enabled { 1 } else { 0 };
        let data = [1, 2, 3, 4, 5, 6, cmd, 0];
        self.send_ext(
            CommunicationType::ACTIVE_REPORT,
            self.host_id_u16(),
            data,
            8,
        )
    }

    pub fn send_cmd_mit(
        &self,
        target_position: f32,
        target_velocity: f32,
        stiffness: f32,
        damping: f32,
        feedforward_torque: f32,
    ) -> Result<()> {
        let (extra_data, data) = encode_mit_command(
            target_position,
            target_velocity,
            stiffness,
            damping,
            feedforward_torque,
            self.limits.p_max,
            self.limits.v_max,
            self.limits.t_max,
            self.kp_max,
            self.kd_max,
        );
        self.send_ext(CommunicationType::OPERATION_CONTROL, extra_data, data, 8)
    }

    pub fn set_velocity_target(&self, velocity: f32) -> Result<()> {
        self.write_parameter(
            ParameterId::VelocityTarget as u16,
            ParameterValue::F32(velocity),
        )
    }

    pub fn write_parameter(&self, param_id: u16, value: ParameterValue) -> Result<()> {
        let raw = encode_parameter_value(param_id, value)?;
        let data = encode_parameter_write(param_id, raw);
        self.send_with_status_ack(
            CommunicationType::WRITE_PARAMETER,
            data,
            8,
            Duration::from_millis(260),
        )
    }

    pub fn request_parameter(&self, param_id: u16) -> Result<()> {
        let mut ps = self
            .param_state
            .lock()
            .map_err(|_| MotorError::Io("param state lock poisoned".to_string()))?;
        ps.values.remove(&param_id);
        ps.pending.replace(param_id);
        drop(ps);
        let data = encode_parameter_read(param_id);
        self.send_ext(
            CommunicationType::READ_PARAMETER,
            self.host_id_u16(),
            data,
            8,
        )
    }

    pub fn get_parameter(&self, param_id: u16, timeout: Duration) -> Result<ParameterValue> {
        let cands = self.host_id_candidates();
        let per_try = Duration::from_millis((timeout.as_millis() as u64).max(150));

        for host in cands {
            if let Ok(value) = self.get_parameter_with_host_id(param_id, host, per_try) {
                return Ok(value);
            }
        }
        Err(MotorError::Timeout(format!(
            "parameter 0x{param_id:04X} not received within {:?}",
            timeout
        )))
    }

    pub fn get_parameter_with_host_id(
        &self,
        param_id: u16,
        host_id: u16,
        timeout: Duration,
    ) -> Result<ParameterValue> {
        Self::validate_host_id(host_id, "feedback_id")?;
        let mut ps = self
            .param_state
            .lock()
            .map_err(|_| MotorError::Io("param state lock poisoned".to_string()))?;
        ps.values.remove(&param_id);
        ps.pending.replace(param_id);
        drop(ps);
        let data = encode_parameter_read(param_id);
        self.send_ext(CommunicationType::READ_PARAMETER, host_id, data, 8)?;

        let deadline = Instant::now() + timeout;
        loop {
            if let Some(value) = self
                .param_state
                .lock()
                .map_err(|_| MotorError::Io("param state lock poisoned".to_string()))?
                .values
                .get(&param_id)
                .copied()
            {
                return Ok(value);
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        Err(MotorError::Timeout(format!(
            "parameter 0x{param_id:04X} not received within {:?} for host_id 0x{host_id:X}",
            timeout
        )))
    }

    pub fn get_parameter_f32(&self, param_id: u16, timeout: Duration) -> Result<f32> {
        match self.get_parameter(param_id, timeout)? {
            ParameterValue::F32(v) => Ok(v),
            _ => Err(MotorError::Protocol(format!(
                "parameter 0x{param_id:04X} is not f32"
            ))),
        }
    }

    pub fn get_parameter_i8(&self, param_id: u16, timeout: Duration) -> Result<i8> {
        match self.get_parameter(param_id, timeout)? {
            ParameterValue::I8(v) => Ok(v),
            _ => Err(MotorError::Protocol(format!(
                "parameter 0x{param_id:04X} is not i8"
            ))),
        }
    }

    pub fn latest_state(&self) -> Option<MotorFeedbackState> {
        self.state.lock().ok().and_then(|s| *s)
    }

    pub fn latest_fault_report(&self) -> Option<FaultReport> {
        self.fault_report.lock().ok().and_then(|s| *s)
    }

    fn process_feedback_frame_impl(&self, frame: CanFrame) -> Result<()> {
        let (comm_type, extra_data, _) = ext_id_parts(frame.arbitration_id);
        match comm_type {
            CommunicationType::GET_DEVICE_ID => {
                let reply = decode_ping_reply(frame.arbitration_id, frame.data)?;
                self.ping_reply
                    .lock()
                    .map_err(|_| MotorError::Io("ping reply lock poisoned".to_string()))?
                    .replace(reply);
                Ok(())
            }
            CommunicationType::READ_PARAMETER => {
                let mut ps = self
                    .param_state
                    .lock()
                    .map_err(|_| MotorError::Io("param state lock poisoned".to_string()))?;
                let param_id = ps
                    .pending
                    .take()
                    .unwrap_or_else(|| u16::from_le_bytes([frame.data[0], frame.data[1]]));
                let raw = decode_read_parameter_value(param_id, frame.data)?;
                let value = if let Some(info) = parameter_info(param_id) {
                    match info.data_type {
                        ParameterDataType::Int8 => ParameterValue::I8(raw[0] as i8),
                        ParameterDataType::UInt8 => ParameterValue::U8(raw[0]),
                        ParameterDataType::UInt16 => {
                            ParameterValue::U16(u16::from_le_bytes([raw[0], raw[1]]))
                        }
                        ParameterDataType::UInt32 => ParameterValue::U32(u32::from_le_bytes(raw)),
                        ParameterDataType::Float32 => ParameterValue::F32(f32::from_le_bytes(raw)),
                    }
                } else {
                    // Tolerate unknown vendor firmware params instead of surfacing hard errors
                    // in polling worker logs. Preserve raw payload as U32 for diagnostics.
                    ParameterValue::U32(u32::from_le_bytes(raw))
                };
                ps.values.insert(param_id, value);
                Ok(())
            }
            CommunicationType::OPERATION_STATUS | CommunicationType::FAULT_REPORT => {
                let status = decode_status_frame(
                    extra_data,
                    frame.data,
                    self.limits.p_max,
                    self.limits.v_max,
                    self.limits.t_max,
                );
                self.state
                    .lock()
                    .map_err(|_| MotorError::Io("state lock poisoned".to_string()))?
                    .replace(MotorFeedbackState {
                        arbitration_id: frame.arbitration_id,
                        device_id: status.flags.device_id,
                        position: status.position,
                        velocity: status.velocity,
                        torque: status.torque,
                        temperature_c: status.temperature_c,
                        uncalibrated: status.flags.uncalibrated,
                        stall: status.flags.stall,
                        magnetic_encoder_fault: status.flags.magnetic_encoder_fault,
                        overtemperature: status.flags.overtemperature,
                        overcurrent: status.flags.overcurrent,
                        undervoltage: status.flags.undervoltage,
                    });
                if comm_type == CommunicationType::FAULT_REPORT {
                    self.fault_report
                        .lock()
                        .map_err(|_| MotorError::Io("fault report lock poisoned".to_string()))?
                        .replace(decode_fault_report(frame.data));
                }
                self.status_seq.fetch_add(1, Ordering::Release);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl MotorDevice for RobstrideMotor {
    fn vendor(&self) -> &'static str {
        "robstride"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn motor_id(&self) -> u16 {
        self.motor_id
    }

    fn feedback_id(&self) -> u16 {
        self.feedback_id
    }

    fn enable(&self) -> Result<()> {
        RobstrideMotor::enable(self)
    }

    fn disable(&self) -> Result<()> {
        RobstrideMotor::disable(self)
    }

    fn accepts_frame(&self, frame: &CanFrame) -> bool {
        if !frame.is_extended {
            return false;
        }
        let (comm_type, extra_data, _responder_id) = ext_id_parts(frame.arbitration_id);
        let device_id = (extra_data & 0xFF) as u16;
        match comm_type {
            CommunicationType::GET_DEVICE_ID => device_id == self.motor_id,
            CommunicationType::READ_PARAMETER => device_id == self.motor_id,
            // Status/fault frames must belong to this motor. Accepting only by responder_id
            // can pollute state with frames from other motors on the same bus.
            CommunicationType::OPERATION_STATUS | CommunicationType::FAULT_REPORT => {
                device_id == self.motor_id
            }
            _ => false,
        }
    }

    fn process_feedback_frame(&self, frame: CanFrame) -> Result<()> {
        self.process_feedback_frame_impl(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motor_core::device::MotorDevice;
    use motor_core::test_support::MockBus;

    #[test]
    fn get_parameter_times_out_when_no_reply_arrives() {
        let bus: Arc<dyn CanBus> = Arc::new(MockBus::new());
        let motor = RobstrideMotor::new(127, 0xFF, "rs-00", bus).expect("create motor");
        let err = motor
            .get_parameter(0x7019, Duration::from_millis(5))
            .expect_err("timeout expected");
        assert!(matches!(err, MotorError::Timeout(_)));
    }

    #[test]
    fn constructor_rejects_out_of_range_ids() {
        let bus: Arc<dyn CanBus> = Arc::new(MockBus::new());
        assert!(RobstrideMotor::new(0, 0xFD, "rs-00", Arc::clone(&bus)).is_err());
        assert!(RobstrideMotor::new(256, 0xFD, "rs-00", Arc::clone(&bus)).is_err());
        assert!(RobstrideMotor::new(1, 0x100, "rs-00", bus).is_err());
    }

    #[test]
    fn ping_with_host_id_uses_exact_host_without_fallback() {
        let bus = Arc::new(MockBus::new());
        let motor = RobstrideMotor::new(2, 0xFD, "rs-00", bus.clone()).expect("create motor");
        let err = motor
            .ping_with_host_id(0xAA, Duration::from_millis(1))
            .expect_err("timeout expected");
        assert!(matches!(err, MotorError::Timeout(_)));

        let sent = bus.sent.lock().expect("sent frames");
        assert_eq!(sent.len(), 1);
        let (comm_type, extra_data, node_id) = ext_id_parts(sent[0].arbitration_id);
        assert_eq!(comm_type, CommunicationType::GET_DEVICE_ID);
        assert_eq!(extra_data, 0x00AA);
        assert_eq!(node_id, 2);
    }

    #[test]
    fn read_parameter_filter_rejects_other_device_with_same_host() {
        let bus: Arc<dyn CanBus> = Arc::new(MockBus::new());
        let motor = RobstrideMotor::new(2, 0xFD, "rs-00", bus).expect("create motor");
        let frame = CanFrame {
            arbitration_id: build_ext_id(CommunicationType::READ_PARAMETER, 0x0003, 0xFD),
            data: encode_parameter_read(0x7019),
            dlc: 8,
            is_extended: true,
            is_rx: true,
        };
        assert!(!motor.accepts_frame(&frame));
    }
}
