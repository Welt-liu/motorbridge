use crate::commands::{cmd_scan, cmd_set_id, cmd_verify};
use crate::router::stream::RobstrideParamStream;
use crate::session::SessionCtx;
use serde_json::Value;

use super::handlers;

pub(crate) fn dispatch_op(
    op: &str,
    v: &Value,
    ctx: &mut SessionCtx,
    state_stream_enabled: &mut bool,
    robstride_param_stream: &mut RobstrideParamStream,
    dt_ms: u64,
) -> Result<serde_json::Value, String> {
    if let Some(r) = handlers::connection::handle(
        op,
        v,
        ctx,
        state_stream_enabled,
        robstride_param_stream,
        dt_ms,
    ) {
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
        "scan" => cmd_scan(v, &ctx.target),
        "set_id" => cmd_set_id(v, &ctx.target),
        "verify" => cmd_verify(v, &ctx.target),
        _ => Err(format!("unsupported op: {op}")),
    }
}
