use std::{collections::HashMap, str::FromStr, time::Duration};
use base64::Engine;

use actix_web::{error, web, HttpResponse, Responder};
use async_channel::Receiver;
use crossbeam::channel::Sender as CBSender;
use serde::{Deserialize, Serialize};
use solana_sdk::{keccak::Hash, transaction::Transaction};
use tokio::time::timeout;
use solana_client::nonblocking::rpc_client::RpcClient; 
use solana_sdk::{
    message::Message,
    signature::Signer,
    commitment_config::CommitmentConfig,
};
use solana_system_interface::instruction as system_instruction;

use crate::rollupdb::RollupDBMessage;

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
    pub sol_transaction: Option<String>, // Changed to String for base64 input
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RollupTransactionResponse {
    pub sender: Option<String>,
    pub sol_transaction: Option<Transaction>, // Keep Transaction for responses
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

async fn recv_once<T>(rx: &Receiver<T>, dur: Duration) -> Option<T> {
    timeout(dur, rx.recv()).await.ok().and_then(Result::ok)
}

fn ok_json<T: Serialize>(v: T) -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(v))
}

fn err_json(msg: &str) -> actix_web::Result<HttpResponse> {
    ok_json(HashMap::from([("error", msg)]))
}

pub async fn submit_transaction(
    body: web::Json<RollupTransaction>,
    sequencer_sender: web::Data<CBSender<Transaction>>,
) -> actix_web::Result<impl Responder> {
    log::info!("Submitted transaction");
    log::info!("Json({:?})", body);

    match body.sol_transaction.clone() {
        Some(tx_base64) => {
            // Decode base64 string to Transaction
            match base64::engine::general_purpose::STANDARD.decode(&tx_base64) {
                Ok(tx_bytes) => {
                    match bincode::deserialize::<Transaction>(&tx_bytes) {
                        Ok(tx) => {
                            match sequencer_sender.send(tx) {
                                Ok(_) => Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "Submitted")]))),
                                Err(e) => {
                                    log::error!("Failed to send transaction to sequencer: {}", e);
                                    Ok(HttpResponse::InternalServerError().json(HashMap::from([
                                        ("error", "Failed to submit transaction to sequencer")
                                    ])))
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to deserialize transaction: {}", e);
                            Ok(HttpResponse::BadRequest().json(HashMap::from([
                                ("error", "Invalid transaction format")
                            ])))
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to decode base64 transaction: {}", e);
                    Ok(HttpResponse::BadRequest().json(HashMap::from([
                        ("error", "Invalid base64 transaction")
                    ])))
                }
            }
        }
        None => {
            log::info!("Creating test transaction for testing");
            let sender_name = body.sender.as_deref().unwrap_or("unknown");
            match create_test_transaction(sender_name).await {
                Ok(dummy_tx) => {
                    sequencer_sender.send(dummy_tx).unwrap();
                    Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "Submitted (test)")])))
                }
                Err(e) => {
                    log::error!("Failed to create test transaction: {}", e);
                    Ok(HttpResponse::BadRequest().json(HashMap::from([("error", format!("Failed to create transaction: {}", e))])))
                }
            }
        }
    }
}

async fn create_test_transaction(_sender: &str) -> Result<Transaction, Box<dyn std::error::Error>> {
    let keypair_path = std::env::var("KEYPAIR2")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", std::env::var("HOME").unwrap()));
    
    let payer = solana_sdk::signer::keypair::read_keypair_file(&keypair_path)?;

    let rpc_client = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".to_string(),
        CommitmentConfig::confirmed(),
    );

    let recent_blockhash = rpc_client.get_latest_blockhash().await?;
    
    log::info!("Creating test transaction: {} SOL transfer from {} to themselves", 0.001, payer.pubkey());
    
    let instruction = system_instruction::transfer(
        &payer.pubkey(),
        &payer.pubkey(), 
        1_000_000, 
    );
    
    let message = Message::new(&[instruction], Some(&payer.pubkey()));
    
    let mut transaction = Transaction::new_unsigned(message);
    transaction.sign(&[&payer], recent_blockhash);
    
    Ok(transaction)
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
            store_batch_proof: None,
            update_proof_status: None,
            get_proof_by_batch_id: None,
            get_unsettled_proofs: None,
            retry_failed_proofs: None,
            list_offset: None,
            list_limit: None,
            trigger_retry_cycle: None,
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

                return ok_json(RollupTransactionResponse {
                    sender: Some(sender),
                    sol_transaction: Some(tx), // raw tx
                    error: None,
                });
            } else if let Some(err) = frontend_message.error {
                return ok_json(RollupTransactionResponse {
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
        store_batch_proof: None,
        update_proof_status: None,
        get_proof_by_batch_id: None,
        get_unsettled_proofs: None,
        retry_failed_proofs: None,
        list_offset: Some(offset),
        list_limit: Some(per_page),
        trigger_retry_cycle: None,
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