use crate::commands::{cmd_batch_scan, cmd_scan, cmd_set_id, cmd_verify, parse_vendor_in_msg};
use crate::model::{ControllerHandle, Vendor};
use crate::router::stream::ParamStream;
use crate::session::SessionCtx;
use serde_json::Value;

use super::handlers;

pub(crate) fn dispatch_op(
    op: &str,
    v: &Value,
    ctx: &mut SessionCtx,
    state_stream_enabled: &mut bool,
    param_stream: &mut ParamStream,
    dt_ms: u64,
) -> Result<serde_json::Value, String> {
    if let Some(r) =
        handlers::connection::handle(op, v, ctx, state_stream_enabled, param_stream, dt_ms)
    {
        return r;
    }
    if let Some(r) = handlers::control::handle(op, v, ctx) {
        return r;
    }
    if let Some(r) = handlers::control_aux::handle(op, v, ctx) {
        return r;
    }
    if let Some(r) = handlers::register::handle(op, v, ctx) {
        return r;
    }

    match op {
        "batch_scan" => {
            release_robstride_session_before_scan(v, ctx);
            cmd_batch_scan(v, &ctx.target)
        }
        "scan" => {
            release_robstride_session_before_scan(v, ctx);
            cmd_scan(v, &ctx.target)
        }
        "set_id" => cmd_set_id(v, &ctx.target),
        "verify" => cmd_verify(v, &ctx.target),
        _ => Err(format!("unsupported op: {op}")),
    }
}

pub(crate) fn release_robstride_session_before_scan(v: &Value, ctx: &mut SessionCtx) {
    if !scan_request_may_include_robstride(v, ctx.target.vendor) {
        return;
    }
    if matches!(ctx.controller, Some(ControllerHandle::Robstride(_))) {
        ctx.disconnect(false);
        #[cfg(target_os = "windows")]
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn scan_request_may_include_robstride(v: &Value, default_vendor: Vendor) -> bool {
    if parse_vendor_in_msg(v, default_vendor).ok() == Some(Vendor::Robstride) {
        return true;
    }
    for key in ["vendors", "scans", "requests", "items"] {
        let Some(items) = v.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            if item.as_str().map(|s| s.eq_ignore_ascii_case("robstride")) == Some(true) {
                return true;
            }
            if item
                .get("vendor")
                .and_then(Value::as_str)
                .map(|s| s.eq_ignore_ascii_case("robstride"))
                == Some(true)
            {
                return true;
            }
        }
    }
    false
}
