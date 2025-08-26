// src/rpc.rs
use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use crossbeam::channel::Sender as CBSender;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{
    hash::Hash as SolanaHash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction, // legacy
    // versioned_transaction::VersionedTransaction, // enable if you want v0
};
use tokio::time::{timeout, Duration};

use crate::rollupdb::RollupDBMessage;
use crate::frontend::FrontendMessage;

// ---------- Shared state: provide current blockhash/slot info ----------
#[derive(Clone)]
pub struct BlockInfo {
    pub blockhash: SolanaHash,
    pub slot: u64,
    pub last_valid_block_height: u64,
    pub block_height: u64,
}

#[derive(Clone)]
pub struct RpcContext {
    pub sequencer_tx: CBSender<Transaction>,
    // pub sequencer_tx_v0: CBSender<VersionedTransaction>, // if you switch
    pub rollupdb_tx: CBSender<RollupDBMessage>,
    pub frontend_rx: async_channel::Receiver<FrontendMessage>,
    pub blockinfo: parking_lot::RwLock<BlockInfo>,
}

// ---------- JSON-RPC wire types ----------
#[derive(Deserialize)]
#[serde(untagged)]
enum RpcCall {
    Single(RpcRequest),
    Batch(Vec<RpcRequest>),
}

#[derive(Deserialize)]
struct RpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn ok(id: Option<Value>, result: Value) -> RpcResponse {
    RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}
fn err(id: Option<Value>, code: i64, msg: impl Into<String>, data: Option<Value>) -> RpcResponse {
    RpcResponse { jsonrpc: "2.0", id, result: None, error: Some(RpcError { code, message: msg.into(), data }) }
}

// ---------- Helpers ----------
fn parse_string(params: &Value, idx: usize) -> Result<String, String> {
    params.get(idx)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("param[{idx}] must be string"))
}
fn parse_vec_strings(params: &Value, idx: usize) -> Result<Vec<String>, String> {
    params.get(idx)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .ok_or_else(|| format!("param[{idx}] must be array of strings"))
}

// ---------- Method implementations ----------
async fn method_get_version(ctx: &RpcContext, id: Option<Value>) -> RpcResponse {
    let bi = ctx.blockinfo.read();
    ok(id, json!({"solana-core": "rollup-0.1", "feature-set": 0, "slot": bi.slot}))
}

async fn method_get_latest_blockhash(ctx: &RpcContext, id: Option<Value>) -> RpcResponse {
    let bi = ctx.blockinfo.read();
    ok(id, json!({
        "context": { "slot": bi.slot },
        "value": {
            "blockhash": bi.blockhash.to_string(),
            "lastValidBlockHeight": bi.last_valid_block_height
        }
    }))
}

async fn method_is_blockhash_valid(ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    let s = match parse_string(&params, 0) {
        Ok(v) => v,
        Err(e) => return err(id, -32602, e, None),
    };
    let parsed = SolanaHash::from_str(&s).ok();
    let bi = ctx.blockinfo.read();
    let valid = parsed.is_some() && s == bi.blockhash.to_string();
    ok(id, json!({ "context": { "slot": bi.slot }, "value": valid }))
}

async fn method_send_transaction(ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    // params: [ base64_tx, {encoding:"base64", ...}? ]
    let b64 = match parse_string(&params, 0) {
        Ok(v) => v,
        Err(e) => return err(id, -32602, e, None),
    };

    let bytes = match B64.decode(b64) {
        Ok(x) => x,
        Err(e) => return err(id, -32602, "invalid base64", Some(json!({"detail": e.to_string()}))),
    };

    // Try legacy Transaction
    let tx: Transaction = match bincode::deserialize(&bytes) {
        Ok(t) => t,
        Err(e) => {
            // If you want VersionedTransaction support:
            // let vtx: VersionedTransaction = match bincode::deserialize(&bytes) {
            //     Ok(v) => v,
            //     Err(_) => return err(id, -32602, "unsupported transaction format", None),
            // };
            // ctx.sequencer_tx_v0.send(vtx).map_err(...);
            return err(id, -32602, "only legacy Transaction supported for now", Some(json!({"detail": e.to_string()})));
        }
    };

    if tx.signatures.is_empty() {
        return err(id, -32000, "missing signature", None);
    }

    let sig_b58 = Signature::new(&tx.signatures[0].0).to_string();
    if let Err(e) = ctx.sequencer_tx.send(tx) {
        return err(id, -32000, "enqueue failed", Some(json!({"detail": e.to_string()})));
    }
    ok(id, json!(sig_b58))
}

async fn method_get_signature_statuses(ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    // params: [ [sig1, sig2, ...], opts? ]
    let sigs = match parse_vec_strings(&params, 0) {
        Ok(v) => v,
        Err(e) => return err(id, -32602, e, None),
    };

    // For now, query your DB for just presence via the existing single-tx path.
    // If found -> processed; refine to confirmed/finalized as your rollup adds status.
    let mut out = Vec::with_capacity(sigs.len());
    for s in sigs {
        let wanted = match SolanaHash::from_str(&s) {
            Ok(h) => h,
            Err(_) => { out.push(Value::Null); continue; }
        };

        let _ = ctx.rollupdb_tx.send(RollupDBMessage {
            lock_accounts: None,
            add_processed_transaction: None,
            frontend_get_tx: Some(wanted),
            add_settle_proof: None,
            add_new_data: None,
            list_offset: None,
            list_limit: None,
        });

        let resp = timeout(Duration::from_millis(800), ctx.frontend_rx.recv()).await;
        match resp {
            Ok(Ok(FrontendMessage { transaction: Some(_tx), .. })) => {
                out.push(json!({
                    "slot": null,
                    "confirmations": null,
                    "err": null,
                    "confirmationStatus": "processed"
                }));
            }
            _ => out.push(Value::Null), // unknown/unavailable
        }
    }

    let bi = ctx.blockinfo.read();
    ok(id, json!({ "context": { "slot": bi.slot }, "value": out }))
}

async fn method_get_balance(ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    // You likely have an accounts DB; return 0 for now
    let _pk = match parse_string(&params, 0)
        .and_then(|s| Pubkey::from_str(&s).map_err(|_| "invalid pubkey".to_string())) {
        Ok(_) => (),
        Err(e) => return err(id, -32602, e, None),
    };
    let bi = ctx.blockinfo.read();
    ok(id, json!({ "context": { "slot": bi.slot }, "value": 0u64 }))
}

async fn method_get_account_info(ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    let _pk = match parse_string(&params, 0)
        .and_then(|s| Pubkey::from_str(&s).map_err(|_| "invalid pubkey".to_string())) {
        Ok(pk) => pk,
        Err(e) => return err(id, -32602, e, None),
    };
    let bi = ctx.blockinfo.read();
    // Minimal empty account; replace with real lookup
    ok(id, json!({
        "context": { "slot": bi.slot },
        "value": {
            "lamports": 0,
            "owner": "11111111111111111111111111111111",
            "executable": false,
            "rentEpoch": 0,
            "data": ["", "base64"]
        }
    }))
}

async fn method_get_min_rent(_ctx: &RpcContext, id: Option<Value>, params: Value) -> RpcResponse {
    // params: [ dataLen ]
    let _data_len = match params.get(0).and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => return err(id, -32602, "param[0] must be u64", None),
    };
    ok(id, json!(0u64)) // rent-free rollup; adjust if you charge rent
}

async fn method_get_slot(ctx: &RpcContext, id: Option<Value>) -> RpcResponse {
    let bi = ctx.blockinfo.read();
    ok(id, json!(bi.slot))
}
async fn method_get_block_height(ctx: &RpcContext, id: Option<Value>) -> RpcResponse {
    let bi = ctx.blockinfo.read();
    ok(id, json!(bi.block_height))
}

// ---------- Main handler ----------
pub async fn rpc_handler(
    ctx: web::Data<RpcContext>,
    body: web::Bytes,
) -> actix_web::Result<HttpResponse> {
    let text = String::from_utf8(body.to_vec()).unwrap_or_default();
    let parsed: Result<RpcCall, _> = serde_json::from_str(&text);

    let responses = match parsed {
        Ok(RpcCall::Single(req)) => vec![dispatch(&ctx, req).await],
        Ok(RpcCall::Batch(calls)) => {
            let mut out = Vec::with_capacity(calls.len());
            for req in calls {
                out.push(dispatch(&ctx, req).await);
            }
            out
        }
        Err(_) => vec![err(None, -32700, "parse error", None)],
    };

    // If single and it's a "notification" (no id) we still return 200 with empty array for simplicity.
    Ok(HttpResponse::Ok().json(if responses.len() == 1 { &responses[0] } else { &responses }))
}

async fn dispatch(ctx: &RpcContext, req: RpcRequest) -> RpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "getVersion" => method_get_version(ctx, id).await,
        "getLatestBlockhash" => method_get_latest_blockhash(ctx, id).await,
        "isBlockhashValid" => method_is_blockhash_valid(ctx, id, req.params).await,
        "sendTransaction" => method_send_transaction(ctx, id, req.params).await,
        "getSignatureStatuses" => method_get_signature_statuses(ctx, id, req.params).await,
        "getBalance" => method_get_balance(ctx, id, req.params).await,
        "getAccountInfo" => method_get_account_info(ctx, id, req.params).await,
        "getMinimumBalanceForRentExemption" => method_get_min_rent(ctx, id, req.params).await,
        "getSlot" => method_get_slot(ctx, id).await,
        "getBlockHeight" => method_get_block_height(ctx, id).await,

        _ => err(id, -32601, "method not found", Some(json!({"method": req.method}))),
    }
}
