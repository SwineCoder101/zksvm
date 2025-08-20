use std::{collections::HashMap, str::FromStr, time::Duration};

use actix_web::{error, web, HttpResponse, Responder};
use async_channel::Receiver;
use crossbeam::channel::Sender as CBSender;
use serde::{Deserialize, Serialize};
use solana_sdk::{keccak::Hash, transaction::Transaction};
use tokio::time::timeout;

use crate::rollupdb::RollupDBMessage;

// === Types ===

pub struct FrontendMessage {
    pub get_tx: Option<Hash>,
    pub transaction: Option<Transaction>,                 // single
    pub transactions: Option<Vec<TransactionWithHash>>,  // list
    pub total: Option<u64>,
    pub has_more: Option<bool>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetTransaction {
    /// If present → fetch a single tx by this base58 hash.
    pub get_tx: Option<String>,
    /// For list mode:
    pub page: Option<u32>,     // 1-based
    pub per_page: Option<u32>, // default 50, max 500
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RollupTransaction {
    pub sender: Option<String>,
    pub sol_transaction: Option<Transaction>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransactionWithHash {
    pub hash: String,
    pub transaction: Transaction,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RollupTransactionsList {
    pub sender: Option<String>,
    pub transactions: Vec<TransactionWithHash>, // raw transactions
    pub page: u32,
    pub per_page: u32,
    pub total: Option<u64>,
    pub has_more: bool,
    pub error: Option<String>,
}

// === Helpers ===

async fn recv_once<T>(rx: &Receiver<T>, dur: Duration) -> Option<T> {
    timeout(dur, rx.recv()).await.ok().and_then(Result::ok)
}

fn ok_json<T: Serialize>(v: T) -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(v))
}

fn err_json(msg: &str) -> actix_web::Result<HttpResponse> {
    ok_json(HashMap::from([("error", msg)]))
}

// === Handlers ===

pub async fn submit_transaction(
    body: web::Json<RollupTransaction>,
    sequencer_sender: web::Data<CBSender<Transaction>>,
) -> actix_web::Result<impl Responder> {
    log::info!("Submitted transaction: {:?}", body);

    match body.sol_transaction.clone() {
        Some(tx) => {
            if let Err(e) = sequencer_sender.send(tx) {
                log::error!("Failed to enqueue tx: {e}");
                return err_json("Failed to enqueue transaction");
            }
            ok_json(HashMap::from([("Transaction status", "Submitted")]))
        }
        None => {
            log::warn!("Submit request missing sol_transaction");
            Ok(HttpResponse::BadRequest()
                .json(HashMap::from([("error", "Missing sol_transaction")])))
        }
    }
}

pub async fn get_transaction(
    body: web::Json<GetTransaction>,
    rollupdb_sender: web::Data<CBSender<RollupDBMessage>>,
    frontend_receiver: web::Data<Receiver<FrontendMessage>>,
) -> actix_web::Result<impl Responder> {
    log::info!("Requested transaction: {:?}", body);

    // === CASE A: specific hash supplied => return single tx (raw) ===
    if let Some(sig) = body
        .get_tx
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let wanted_hash =
            Hash::from_str(sig).map_err(|_| error::ErrorBadRequest("Invalid hash format"))?;

        if let Err(e) = rollupdb_sender.send(RollupDBMessage {
            lock_accounts: None,
            add_processed_transaction: None,
            frontend_get_tx: Some(wanted_hash),
            add_settle_proof: None,
            add_new_data: None,
            list_offset: None,
            list_limit: None,
        }) {
            log::error!("Failed to request specific tx: {e}");
            return err_json("Backend request failed");
        }

        if let Some(frontend_message) = recv_once(&frontend_receiver, Duration::from_secs(2)).await
        {
            if let Some(tx) = frontend_message.transaction {
                let sender = tx
                    .message
                    .account_keys
                    .get(0)
                    .map(|k| k.to_string())
                    .unwrap_or_else(|| "unknown".into());

                return ok_json(RollupTransaction {
                    sender: Some(sender),
                    sol_transaction: Some(tx), // raw tx
                    error: None,
                });
            } else if let Some(err) = frontend_message.error {
                return ok_json(RollupTransaction {
                    sender: None,
                    sol_transaction: None,
                    error: Some(err),
                });
            }
        }

        // Fallback if nothing arrives in time
        return ok_json(HashMap::from([("Transaction status", "requested")]));
    }

    // === CASE B: no hash => return paginated list (raw) ===
    let per_page: u32 = body.per_page.unwrap_or(50).clamp(1, 500); // hardcap
    let page: u32 = body.page.unwrap_or(1).max(1);
    let offset: u64 = (page as u64 - 1) * per_page as u64;

    if let Err(e) = rollupdb_sender.send(RollupDBMessage {
        lock_accounts: None,
        add_processed_transaction: None,
        frontend_get_tx: None, // list mode
        add_settle_proof: None,
        add_new_data: None,
        list_offset: Some(offset),
        list_limit: Some(per_page),
    }) {
        log::error!("Failed to request paged list from RollupDB: {e}");
        return ok_json(RollupTransactionsList {
            sender: None,
            transactions: vec![],
            page,
            per_page,
            total: None,
            has_more: false,
            error: Some("Backend request failed".into()),
        });
    }

    if let Some(msg) = recv_once(&frontend_receiver, Duration::from_secs(2)).await {
        if let Some(list) = msg.transactions {
            // Prefer DB-provided has_more; else infer from total if present.
            let total = msg.total;
            let has_more = msg
                .has_more
                .unwrap_or_else(|| total.map(|t| offset + (list.len() as u64) < t).unwrap_or(false));

            return ok_json(RollupTransactionsList {
                sender: None,
                transactions: list, // raw txs
                page,
                per_page,
                total,
                has_more,
                error: None,
            });
        } else if let Some(err) = msg.error {
            return ok_json(RollupTransactionsList {
                sender: None,
                transactions: vec![],
                page,
                per_page,
                total: None,
                has_more: false,
                error: Some(err),
            });
        }
    }

    // Timeout / no response
    ok_json(RollupTransactionsList {
        sender: None,
        transactions: vec![],
        page,
        per_page,
        total: None,
        has_more: false,
        error: Some("Timeout waiting for backend".into()),
    })
}

pub async fn test() -> impl Responder {
    log::info!("Test request");
    HttpResponse::Ok().json(HashMap::from([("test", "success")]))
}
