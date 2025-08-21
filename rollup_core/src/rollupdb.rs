use async_channel::Sender as ASender;
use crossbeam::channel::Receiver as CBReceiver;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::AccountSharedData, keccak::Hash, pubkey::Pubkey, transaction::Transaction,
};
use std::{
    collections::HashMap,
    time::SystemTime,
};

use crate::frontend::FrontendMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofData {
    pub pi_a: [String; 3],
    pub pi_b: [[String; 2]; 3],
    pub pi_c: [String; 3],
    pub protocol: String,
    pub curve: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProofStatus {
    Generated,
    Posted,     
    Verified,   
    Failed,    
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProofRecord {
    pub batch_id: String,
    pub proof_data: ProofData,
    pub public_inputs: Vec<String>,
    pub transaction_signatures: Vec<String>,
    pub status: ProofStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub retry_count: u32,
    pub error_message: Option<String>,
}

pub struct RollupDBMessage {
    pub lock_accounts: Option<Vec<Pubkey>>,
    pub add_processed_transaction: Option<Transaction>,
    pub frontend_get_tx: Option<Hash>,
    pub list_offset: Option<u64>,
    pub list_limit: Option<u32>,
    pub add_settle_proof: Option<String>,
    pub add_new_data: Option<Vec<(Pubkey, AccountSharedData)>>,
    pub store_batch_proof: Option<StoreBatchProofMessage>,
    pub update_proof_status: Option<UpdateProofStatusMessage>,
    pub get_proof_by_batch_id: Option<String>,
    pub get_unsettled_proofs: Option<bool>,
    pub retry_failed_proofs: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct StoreBatchProofMessage {
    pub batch_id: String,
    pub proof_data: ProofData,
    pub public_inputs: Vec<String>,
    pub transaction_signatures: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateProofStatusMessage {
    pub batch_id: String,
    pub new_status: ProofStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Default)]
pub struct RollupDB {
    accounts_db: HashMap<Pubkey, AccountSharedData>,
    locked_accounts: HashMap<Pubkey, AccountSharedData>,
    transactions: HashMap<Hash, Transaction>,
    batch_proofs: HashMap<String, BatchProofRecord>, 
    proof_by_transaction: HashMap<String, String>,  
}

impl RollupDB {
    pub async fn run(
        rollup_db_receiver: CBReceiver<RollupDBMessage>,
        frontend_sender: ASender<FrontendMessage>,
        account_sender: ASender<Option<Vec<(Pubkey, AccountSharedData)>>>,
    ) {
        let mut db = RollupDB::default();
        let rpc_client = RpcClient::new("https://api.devnet.solana.com".to_string());

        while let Ok(msg) = rollup_db_receiver.recv() {
            log::info!("RollupDB received a message");

            // Lock + fetch accounts 
            if let Some(accounts_to_lock) = msg.lock_accounts {
                log::info!("DB: Received request to lock and fetch {} accounts.", accounts_to_lock.len());
                let mut fetched: Vec<(Pubkey, AccountSharedData)> = Vec::with_capacity(accounts_to_lock.len());
                
                for pubkey in accounts_to_lock {
                    let account_data = db.accounts_db.remove(&pubkey).or_else(|| {
                        log::warn!("Account {} not in local DB, fetching from L1.", pubkey);
                        rpc_client.get_account(&pubkey).ok().map(|acc| acc.into())
                    });

                    if let Some(data) = account_data {
                        db.locked_accounts.insert(pubkey, data.clone());
                        fetched.push((pubkey, data));
                    } else {
                        log::error!("FATAL: Could not load account {} from L1.", pubkey);
                    }
                }

                log::info!(
                    "DB: Sending {} accounts to sequencer.",
                    fetched.len()
                );
                account_sender
                    .send(Some(fetched))
                    .await
                    .unwrap();
            }
            else if let (Some(tx), Some(new_data)) =
                (msg.add_processed_transaction, msg.add_new_data)
            {
                log::info!("DB: Received processed transaction. Updating state.");

                for (pubkey, account_data) in new_data {
                    db.accounts_db.insert(pubkey, account_data);
                }
                for pubkey in tx.message.account_keys.iter() {
                    db.locked_accounts.remove(pubkey);
                }
                // keccak(hashv) of signature string bytes
                let tx_hash = solana_sdk::keccak::hashv(&[tx.signatures[0].to_string().as_bytes()]);
                db.transactions.insert(tx_hash, tx);
                log::info!(
                    "State update complete. Locked: {}, Unlocked: {}.",
                    db.locked_accounts.len(),
                    db.accounts_db.len()
                );
            }
            else if let Some(get_this_hash_tx) = msg.frontend_get_tx {
                log::info!(
                    "Received request from frontend for tx hash: {}",
                    get_this_hash_tx
                );
                
                if let Some(req_tx) = db.transactions.get(&get_this_hash_tx) {
                    log::info!("Found transaction for hash: {}", get_this_hash_tx);
                    frontend_sender
                        .send(FrontendMessage {
                            get_tx: Some(get_this_hash_tx),
                            transaction: Some(req_tx.clone()),
                            transactions: None,
                            total: None,
                            has_more: None,
                            error: None,
                        })
                        .await
                        .unwrap();
                } else {
                    log::warn!("No transaction found for hash: {}", get_this_hash_tx);
                    frontend_sender
                        .send(FrontendMessage {
                            get_tx: Some(get_this_hash_tx),
                            transaction: None,
                            transactions: None,
                            total: None,
                            has_more: None,
                            error: Some("Transaction not found".to_string()),
                        })
                        .await
                        .unwrap();
                }
            }
            else if let Some(store_proof) = msg.store_batch_proof {
                log::info!("DB: Storing batch proof for batch_id: {}", store_proof.batch_id);
                
                let now = SystemTime::now();
                let proof_record = BatchProofRecord {
                    batch_id: store_proof.batch_id.clone(),
                    proof_data: store_proof.proof_data,
                    public_inputs: store_proof.public_inputs,
                    transaction_signatures: store_proof.transaction_signatures.clone(),
                    status: ProofStatus::Generated,
                    created_at: now,
                    updated_at: now,
                    retry_count: 0,
                    error_message: None,
                };

                db.batch_proofs.insert(store_proof.batch_id.clone(), proof_record);

                // here we create transaction signature -> batch_id mapping for quick lookup
                for tx_sig in store_proof.transaction_signatures {
                    db.proof_by_transaction.insert(tx_sig, store_proof.batch_id.clone());
                }

                log::info!("Batch proof stored successfully. Total proofs: {}", db.batch_proofs.len());
            }
            else if let Some(update_status) = msg.update_proof_status {
                log::info!("DB: Updating proof status for batch_id: {} to {:?}", 
                          update_status.batch_id, update_status.new_status);

                if let Some(proof_record) = db.batch_proofs.get_mut(&update_status.batch_id) {
                    proof_record.status = update_status.new_status;
                    proof_record.updated_at = SystemTime::now();
                    proof_record.error_message = update_status.error_message;
                    if proof_record.status == ProofStatus::Failed {
                        proof_record.retry_count += 1;
                    }

                    log::info!("Proof status updated successfully");
                } else {
                    log::error!("Batch proof not found for batch_id: {}", update_status.batch_id);
                }
            }
            else if let Some(batch_id) = msg.get_proof_by_batch_id {
                log::info!("DB: Looking up proof for batch_id: {}", batch_id);
                
                if let Some(proof_record) = db.batch_proofs.get(&batch_id) {
                    log::info!("Found proof for batch_id: {} with status: {:?}", 
                              batch_id, proof_record.status);
                    // TODO: Send proof back through a channel 
                } else {
                    log::warn!("No proof found for batch_id: {}", batch_id);
                }
            }
            else if let Some(_get_unsettled) = msg.get_unsettled_proofs {
                let unsettled_proofs: Vec<&BatchProofRecord> = db.batch_proofs
                    .values()
                    .filter(|proof| matches!(proof.status, ProofStatus::Generated | ProofStatus::Posted | ProofStatus::Failed))
                    .collect();
                log::info!("DB: Found {} unsettled proofs", unsettled_proofs.len());
                
                for proof in &unsettled_proofs {
                    log::info!("  - Batch ID: {}, Status: {:?}, Retry Count: {}", 
                              proof.batch_id, proof.status, proof.retry_count);
                }
                // TODO: Send unsettled proofs back through a channel 
            }
            else if let Some(_retry_failed) = msg.retry_failed_proofs {
                let failed_proofs: Vec<String> = db.batch_proofs
                    .iter()
                    .filter(|(_, proof)| proof.status == ProofStatus::Failed && proof.retry_count < 3)
                    .map(|(batch_id, _)| batch_id.clone())
                    .collect();

                log::info!("DB: Found {} failed proofs eligible for retry", failed_proofs.len());
                
                for batch_id in failed_proofs {
                    if let Some(proof_record) = db.batch_proofs.get_mut(&batch_id) {
                        proof_record.status = ProofStatus::Generated;
                        proof_record.updated_at = SystemTime::now();
                        log::info!("  - Reset batch_id: {} for retry (attempt {})", 
                                  batch_id, proof_record.retry_count + 1);
                    }
                }
            }
        }
    }
}

impl ProofData {
    pub fn from_json_file(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file_content = std::fs::read_to_string(file_path)?;
        let json_value: serde_json::Value = serde_json::from_str(&file_content)?;
        
        Ok(ProofData {
            pi_a: [
                json_value["pi_a"][0].as_str().unwrap().to_string(),
                json_value["pi_a"][1].as_str().unwrap().to_string(),
                json_value["pi_a"][2].as_str().unwrap().to_string(),
            ],
            pi_b: [
                [
                    json_value["pi_b"][0][0].as_str().unwrap().to_string(),
                    json_value["pi_b"][0][1].as_str().unwrap().to_string(),
                ],
                [
                    json_value["pi_b"][1][0].as_str().unwrap().to_string(),
                    json_value["pi_b"][1][1].as_str().unwrap().to_string(),
                ],
                [
                    json_value["pi_b"][2][0].as_str().unwrap().to_string(),
                    json_value["pi_b"][2][1].as_str().unwrap().to_string(),
                ],
            ],
            pi_c: [
                json_value["pi_c"][0].as_str().unwrap().to_string(),
                json_value["pi_c"][1].as_str().unwrap().to_string(),
                json_value["pi_c"][2].as_str().unwrap().to_string(),
            ],
            protocol: json_value["protocol"].as_str().unwrap().to_string(),
            curve: json_value["curve"].as_str().unwrap().to_string(),
        })
    }
}