use crate::commands::{as_bool, as_u64, parse_hex_or_dec};
use serde_json::Value;

pub(crate) const ROBSTRIDE_REALTIME_OBSERVATION_PARAMS: &[u16] = &[
    0x7005, 0x7019, 0x701A, 0x701B, 0x701C, 0x3025, 0x302B, 0x302C,
];

pub(crate) const ROBSTRIDE_FULL_OBSERVATION_PARAMS: &[u16] = &[
    0x300C, 0x300D, 0x300E, 0x300F, 0x3010, 0x3011, 0x3012, 0x3013, 0x3015, 0x3016, 0x3017, 0x3018,
    0x3019, 0x301A, 0x301B, 0x301D, 0x301E, 0x3020, 0x3021, 0x3022, 0x3023, 0x3024, 0x3025, 0x3026,
    0x3027, 0x3028, 0x3029, 0x302A, 0x302B, 0x302C, 0x302D, 0x302E, 0x302F, 0x3030, 0x3031, 0x3033,
    0x3034, 0x3035, 0x3036, 0x3037, 0x3038, 0x3039, 0x303A, 0x303B, 0x303C, 0x303D, 0x303E, 0x303F,
    0x3041, 0x3042, 0x3043, 0x3044, 0x7005, 0x7019, 0x701A, 0x701B, 0x701C,
];

#[derive(Debug, Clone)]
pub(crate) struct RobstrideParamStream {
    pub(crate) enabled: bool,
    pub(crate) tick_div: u64,
    pub(crate) tick_counter: u64,
    pub(crate) timeout_ms: u64,
    pub(crate) params: Vec<u16>,
}

impl Default for RobstrideParamStream {
    fn default() -> Self {
        Self {
            enabled: false,
            tick_div: 50,
            tick_counter: 0,
            timeout_ms: 80,
            params: ROBSTRIDE_REALTIME_OBSERVATION_PARAMS.to_vec(),
        }
    }
}

impl RobstrideParamStream {
    pub(crate) fn apply_message(&mut self, v: &Value, dt_ms: u64) -> Result<(), String> {
        self.enabled = as_bool(v, "enabled", false);
        self.timeout_ms = as_u64(v, "timeout_ms", self.timeout_ms).clamp(20, 1000);
        let interval_ms = as_u64(
            v,
            "interval_ms",
            self.tick_div.saturating_mul(dt_ms).max(dt_ms),
        )
        .clamp(dt_ms.max(1), 10_000);
        self.tick_div = interval_ms.div_ceil(dt_ms.max(1)).max(1);
        self.tick_counter = 0;

        if let Some(params) = parse_param_list(v)? {
            self.params = params;
        } else {
            let profile = v
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("realtime")
                .trim()
                .to_ascii_lowercase();
            self.params = match profile.as_str() {
                "full" | "all" | "observed" | "observation" => {
                    ROBSTRIDE_FULL_OBSERVATION_PARAMS.to_vec()
                }
                _ => ROBSTRIDE_REALTIME_OBSERVATION_PARAMS.to_vec(),
            };
        }

        Ok(())
    }
}

fn parse_param_list(v: &Value) -> Result<Option<Vec<u16>>, String> {
    let Some(raw) = v.get("params").or_else(|| v.get("param_ids")) else {
        return Ok(None);
    };
    let mut out = Vec::new();
    match raw {
        Value::Array(items) => {
            for item in items {
                let id = match item {
                    Value::Number(n) => n.as_u64().map(|x| x as u16),
                    Value::String(s) => Some(parse_hex_or_dec(s)?),
                    _ => None,
                }
                .ok_or_else(|| format!("invalid robstride param id: {item}"))?;
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        Value::String(s) => {
            for part in s.split(',') {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let id = parse_hex_or_dec(trimmed)?;
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        _ => return Err("params must be an array or comma-separated string".to_string()),
    }
    if out.is_empty() {
        return Err("params must not be empty".to_string());
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn robstride_param_stream_uses_realtime_profile_by_default() {
        let mut stream = RobstrideParamStream::default();
        stream
            .apply_message(&json!({"enabled": true, "interval_ms": 1000}), 20)
            .expect("valid stream config");
        assert!(stream.enabled);
        assert_eq!(stream.tick_div, 50);
        assert_eq!(stream.params, ROBSTRIDE_REALTIME_OBSERVATION_PARAMS);
    }

    #[test]
    fn robstride_param_stream_accepts_custom_hex_params() {
        let mut stream = RobstrideParamStream::default();
        stream
            .apply_message(
                &json!({"enabled": true, "params": ["0x7019", "0x701A", 12332, "0x701A"]}),
                20,
            )
            .expect("valid stream config");
        assert_eq!(stream.params, vec![0x7019, 0x701A, 0x302C]);
    }
}
