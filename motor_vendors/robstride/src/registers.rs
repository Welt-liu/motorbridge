#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterDataType {
    Int8,
    Int16,
    Int32,
    UInt8,
    UInt16,
    UInt32,
    Float32,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ParameterId {
    // Manual function-code examples kept for cross-checking the RS00-RS06
    // tables below. They are model-specific factory/diagnostic entries, not
    // part of the common runtime control parameter set from manual section 4.
    // MechanicalOffset = 0x2005,
    // MeasuredPosition = 0x3016,
    // MeasuredVelocity = 0x3017,
    // MeasuredTorque = 0x302C,

    // Common runtime control/readback parameters from the RobStride protocol
    // single-parameter list (0x7005..0x702E). High-level control paths should
    // use these IDs; model-specific 0x2000/0x3000 tables remain below for
    // explicit read/write lookup and diagnostics.
    Mode = 0x7005,
    IqTarget = 0x7006,
    VelocityTarget = 0x700A,
    TorqueLimit = 0x700B,
    CurrentKp = 0x7010,
    CurrentKi = 0x7011,
    CurrentFilterGain = 0x7014,
    PositionTarget = 0x7016,
    VelocityLimit = 0x7017,
    CurrentLimit = 0x7018,
    MechanicalPosition = 0x7019,
    IqFiltered = 0x701A,
    MechanicalVelocity = 0x701B,
    Vbus = 0x701C,
    PositionKp = 0x701E,
    VelocityKp = 0x701F,
    VelocityKi = 0x7020,
    VelocityFilterGain = 0x7021,
    VelocityAccelerationTarget = 0x7022,
    PpVelocityMax = 0x7024,
    PpAccelerationTarget = 0x7025,
    EpscanTime = 0x7026,
    CanTimeout = 0x7028,
    ZeroState = 0x7029,
    Damper = 0x702A,
    AddOffset = 0x702B,
    AlveolousOpen = 0x702C,
    IqTest = 0x702D,
    DccSet = 0x702E,
}

#[derive(Debug, Clone, Copy)]
pub struct ParameterInfo {
    pub id: u16,
    pub name: &'static str,
    pub data_type: ParameterDataType,
}

pub const ROBSTRIDE_PRODUCT_INFO_COMMIT: &str = "ba7236bc26417766fda71e75ae128c66dbd21aba";
pub const ROBSTRIDE_PRODUCT_INFO_URL: &str =
    "https://github.com/RobStride/Product_Information/commit/ba7236bc26417766fda71e75ae128c66dbd21aba";

macro_rules! param {
    ($id:expr, $name:expr, $ty:ident) => {
        ParameterInfo {
            id: $id,
            name: $name,
            data_type: $crate::registers::ParameterDataType::$ty,
        }
    };
}

#[path = "registers_00.rs"]
pub mod registers_00;
#[path = "registers_01.rs"]
pub mod registers_01;
#[path = "registers_02.rs"]
pub mod registers_02;
#[path = "registers_03.rs"]
pub mod registers_03;
#[path = "registers_04.rs"]
pub mod registers_04;
#[path = "registers_05.rs"]
pub mod registers_05;
#[path = "registers_06.rs"]
pub mod registers_06;

pub use registers_00::RS00_PARAMETER_TABLE;
pub use registers_01::RS01_PARAMETER_TABLE;
pub use registers_02::RS02_PARAMETER_TABLE;
pub use registers_03::RS03_PARAMETER_TABLE;
pub use registers_04::RS04_PARAMETER_TABLE;
pub use registers_05::RS05_PARAMETER_TABLE;
pub use registers_06::RS06_PARAMETER_TABLE;

pub static PARAMETER_TABLE: &[ParameterInfo] = &[
    param!(0x7005, "run_mode", Int8),
    param!(0x7006, "iq_ref", Float32),
    param!(0x700A, "spd_ref", Float32),
    param!(0x700B, "limit_torque", Float32),
    param!(0x7010, "cur_kp", Float32),
    param!(0x7011, "cur_ki", Float32),
    param!(0x7014, "cur_filter_gain", Float32),
    param!(0x7016, "loc_ref", Float32),
    param!(0x7017, "limit_spd", Float32),
    param!(0x7018, "limit_cur", Float32),
    param!(0x7019, "mechPos", Float32),
    param!(0x701A, "iqf", Float32),
    param!(0x701B, "mechVel", Float32),
    param!(0x701C, "VBUS", Float32),
    param!(0x701E, "loc_kp", Float32),
    param!(0x701F, "spd_kp", Float32),
    param!(0x7020, "spd_ki", Float32),
    param!(0x7021, "spd_filter_gain", Float32),
    param!(0x7022, "acc_rad", Float32),
    param!(0x7024, "vel_max", Float32),
    param!(0x7025, "acc_set", Float32),
    param!(0x7026, "EPScan_time", UInt16),
    param!(0x7028, "canTimeout", UInt32),
    param!(0x7029, "zero_sta", UInt8),
    param!(0x702A, "damper", UInt8),
    param!(0x702B, "add_offset", Float32),
    param!(0x702C, "alveolous_open", UInt8),
    param!(0x702D, "iq_test", UInt8),
    param!(0x702E, "dcc_set", Float32),
];

pub fn parameter_info(id: u16) -> Option<&'static ParameterInfo> {
    PARAMETER_TABLE.iter().find(|info| info.id == id)
}

pub fn parameter_table_for_model(model: &str) -> &'static [ParameterInfo] {
    match model.trim().to_ascii_lowercase().as_str() {
        "rs-00" | "rs00" => RS00_PARAMETER_TABLE,
        "rs-01" | "rs01" => RS01_PARAMETER_TABLE,
        "rs-02" | "rs02" => RS02_PARAMETER_TABLE,
        "rs-03" | "rs03" => RS03_PARAMETER_TABLE,
        "rs-04" | "rs04" => RS04_PARAMETER_TABLE,
        "rs-05" | "rs05" => RS05_PARAMETER_TABLE,
        "rs-06" | "rs06" => RS06_PARAMETER_TABLE,
        _ => PARAMETER_TABLE,
    }
}

pub fn parameter_info_for_model(model: &str, id: u16) -> Option<&'static ParameterInfo> {
    parameter_table_for_model(model)
        .iter()
        .find(|info| info.id == id)
        .or_else(|| PARAMETER_TABLE.iter().find(|info| info.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tables_keep_only_runtime_control_parameters_active() {
        for model in [
            "rs-00", "rs-01", "rs-02", "rs-03", "rs-04", "rs-05", "rs-06",
        ] {
            assert!(
                parameter_info_for_model(model, 0x2005).is_none(),
                "{model} 0x2005 should stay comment-only"
            );
            assert!(
                parameter_info_for_model(model, 0x3004).is_none(),
                "{model} 0x3004 should stay comment-only"
            );
            assert!(
                parameter_info_for_model(model, 0x302C).is_none(),
                "{model} 0x302C should stay comment-only"
            );
        }
    }

    #[test]
    fn unknown_model_keeps_only_common_control_parameters() {
        assert!(parameter_info_for_model("rs-99", 0x2005).is_none());
        assert!(parameter_info_for_model("rs-99", 0x3004).is_none());
        assert_eq!(
            parameter_info_for_model("rs-99", 0x7019)
                .expect("common mechPos")
                .data_type,
            ParameterDataType::Float32
        );
    }

    #[test]
    fn model_tables_still_resolve_runtime_parameters() {
        for model in [
            "rs-00", "rs-01", "rs-02", "rs-03", "rs-04", "rs-05", "rs-06",
        ] {
            let mode = parameter_info_for_model(model, 0x7005).expect("run_mode");
            assert_eq!(mode.name, "run_mode");
            assert_eq!(mode.data_type, ParameterDataType::Int8);

            let pos = parameter_info_for_model(model, 0x7019).expect("mechPos");
            assert_eq!(pos.name, "mechPos");
            assert_eq!(pos.data_type, ParameterDataType::Float32);

            let dcc = parameter_info_for_model(model, 0x702E).expect("dcc_set");
            assert_eq!(dcc.name, "dcc_set");
            assert_eq!(dcc.data_type, ParameterDataType::Float32);
        }
    }
}
