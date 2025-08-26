use async_channel::Sender as ASender;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::AccountSharedData, keccak::Hash, pubkey::Pubkey, transaction::Transaction,
};
use std::{
    collections::HashMap,
    time::{SystemTime, Duration},
};
use crate::{frontend::{FrontendMessage, TransactionWithHash}, settle::SettlementJob};

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
    pub trigger_retry_cycle: Option<bool>,
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

#[derive(Debug)]
pub struct RollupDB {
    accounts_db: HashMap<Pubkey, AccountSharedData>,
    locked_accounts: HashMap<Pubkey, AccountSharedData>,
    transactions: HashMap<Hash, Transaction>,
    batch_proofs: HashMap<String, BatchProofRecord>, 
    proof_by_transaction: HashMap<String, String>,
    last_retry_cycle: Option<SystemTime>,
    retry_cycle_count: u32,
    consecutive_retry_failures: u32,
}

impl Default for RollupDB {
    fn default() -> Self {
        Self {
            accounts_db: HashMap::new(),
            locked_accounts: HashMap::new(),
            transactions: HashMap::new(),
            batch_proofs: HashMap::new(),
            proof_by_transaction: HashMap::new(),
            last_retry_cycle: None,
            retry_cycle_count: 0,
            consecutive_retry_failures: 0,
        }
    }
}

impl RollupDB {
    // here we check if retry should be allowed
    fn should_allow_retry_cycle(&mut self) -> bool {
        let now = SystemTime::now();
        
        // if there are too many consecutive failures, back off exponentially
        if self.consecutive_retry_failures >= 5 {
            let backoff_seconds = 60 * (1 << self.consecutive_retry_failures.min(8)); 
            let backoff_duration = Duration::from_secs(backoff_seconds);
            
            if let Some(last_retry) = self.last_retry_cycle {
                if now.duration_since(last_retry).unwrap_or(Duration::ZERO) < backoff_duration {
                    log::warn!("DB: Retry cycle blocked by circuit breaker. Consecutive failures: {}. Next retry in {:?}", 
                              self.consecutive_retry_failures, backoff_duration);
                    return false;
                }
            }
        }
        
        // don't retry too frequently
        if let Some(last_retry) = self.last_retry_cycle {
            if now.duration_since(last_retry).unwrap_or(Duration::ZERO) < Duration::from_secs(10) {
                log::debug!("DB: Retry cycle rate limited - last retry was too recent");
                return false;
            }
        }
        self.last_retry_cycle = Some(now);
        self.retry_cycle_count += 1;
        
        log::info!("DB: Retry cycle #{} approved by circuit breaker", self.retry_cycle_count);
        true
    }
    
    fn record_retry_cycle_result(&mut self, success_count: usize, fail_count: usize) {
        if success_count > 0 {
            // success resets the failure counter
            self.consecutive_retry_failures = 0;
            log::info!("DB: Retry cycle had successes - circuit breaker reset");
        } else if fail_count > 0 {
            // failures increment the counter
            self.consecutive_retry_failures += 1;
            log::warn!("DB: Retry cycle had only failures - circuit breaker counter: {}", 
                      self.consecutive_retry_failures);
        }
    }

    pub async fn run(
        rollup_db_receiver: CBReceiver<RollupDBMessage>,
        frontend_sender: ASender<FrontendMessage>,
        account_sender: ASender<Option<Vec<(Pubkey, AccountSharedData)>>>,
        settlement_sender: CBSender<SettlementJob>,
    ) {
        let mut db = RollupDB::default();
        let rpc_client = RpcClient::new("https://api.devnet.solana.com".to_string());
        
        log::info!("RollupDB started with complete retry logic and circuit breaker");

        while let Ok(msg) = rollup_db_receiver.recv() {
            log::debug!("RollupDB received a message");
            if let Some(accounts_to_lock) = msg.lock_accounts {
                log::info!("DB: Locking and fetching {} accounts", accounts_to_lock.len());
                let mut fetched: Vec<(Pubkey, AccountSharedData)> = Vec::with_capacity(accounts_to_lock.len());
                
                for pubkey in accounts_to_lock {
                    let account_data = db.accounts_db.remove(&pubkey).or_else(|| {
                        log::warn!("Account {} not in local DB, fetching from L1", pubkey);
                        rpc_client.get_account(&pubkey).ok().map(|acc| acc.into())
                    });

                    if let Some(data) = account_data {
                        db.locked_accounts.insert(pubkey, data.clone());
                        fetched.push((pubkey, data));
                    } else {
                        log::error!("FATAL: Could not load account {} from L1", pubkey);
                    }
                }

                log::info!("DB: Sending {} accounts to sequencer", fetched.len());
                if let Err(e) = account_sender.send(Some(fetched)).await {
                    log::error!("Failed to send accounts to sequencer: {}", e);
                }
            }
            else if let (Some(tx), Some(new_data)) = (msg.add_processed_transaction, msg.add_new_data) {
                log::info!("DB: Processing transaction state update");

                // we update account states
                for (pubkey, account_data) in new_data {
                    db.accounts_db.insert(pubkey, account_data);
                }
                
                // we unlock accounts that were used in the transaction
                for pubkey in tx.message.account_keys.iter() {
                    db.locked_accounts.remove(pubkey);
                }
                
                // we store transaction with deterministic hash
                let tx_hash = solana_sdk::keccak::hashv(&[tx.signatures[0].to_string().as_bytes()]);
                db.transactions.insert(tx_hash, tx);
                
                log::info!("State update complete. Locked: {}, Available: {}, Total transactions: {}", 
                          db.locked_accounts.len(), db.accounts_db.len(), db.transactions.len());
            }
            // here we perform a single transaction lookup
            else if let Some(get_this_hash_tx) = msg.frontend_get_tx {
                log::info!("Frontend requesting transaction: {}", get_this_hash_tx);
                
                let response = if let Some(req_tx) = db.transactions.get(&get_this_hash_tx) {
                    log::info!("Transaction found: {}", get_this_hash_tx);
                    FrontendMessage {
                        get_tx: Some(get_this_hash_tx),
                        transaction: Some(req_tx.clone()),
                        transactions: None,
                        total: None,
                        has_more: None,
                        error: None,
                    }
                } else {
                    log::warn!("Transaction not found: {}", get_this_hash_tx);
                    FrontendMessage {
                        get_tx: Some(get_this_hash_tx),
                        transaction: None,
                        transactions: None,
                        total: None,
                        has_more: None,
                        error: Some("Transaction not found".to_string()),
                    }
                };
                
                if let Err(e) = frontend_sender.send(response).await {
                    log::error!("Failed to send transaction response to frontend: {}", e);
                }
            }
            else if let (Some(offset), Some(limit)) = (msg.list_offset, msg.list_limit) {
                log::info!("Frontend requesting transaction list: offset={}, limit={}", offset, limit);
                // here we sort by hash descending
                let mut keys: Vec<Hash> = db.transactions.keys().cloned().collect();
                keys.sort_by(|a, b| b.to_string().cmp(&a.to_string()));
                
                let total = keys.len() as u64;
                let offset = offset.min(total) as usize;
                let limit = limit.clamp(1, 500) as usize;
                let end = (offset + limit).min(total as usize);
                
                let txs: Vec<TransactionWithHash> = keys[offset..end]
                    .iter()
                    .filter_map(|h| db.transactions.get(h).map(|tx| TransactionWithHash {
                        hash: h.to_string(),
                        transaction: tx.clone(),
                    }))
                    .collect();
                
                let has_more = (end as u64) < total;
                
                log::info!("Returning {} transactions (page {}-{} of {})", 
                          txs.len(), offset, end, total);
                
                let response = FrontendMessage {
                    get_tx: None,
                    transaction: None,
                    transactions: Some(txs),
                    total: Some(total),
                    has_more: Some(has_more),
                    error: None,
                };
                
                if let Err(e) = frontend_sender.send(response).await {
                    log::error!("Failed to send transaction list to frontend: {}", e);
                }
            }
            else if let Some(store_proof) = msg.store_batch_proof {
                log::info!("DB: Storing batch proof: {}", store_proof.batch_id);
                
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

                // here we create reverse mapping for quick lookup
                for tx_sig in store_proof.transaction_signatures {
                    db.proof_by_transaction.insert(tx_sig, store_proof.batch_id.clone());
                }

                log::info!("Batch proof stored successfully. Total proofs: {}", db.batch_proofs.len());
            }
            else if let Some(update_status) = msg.update_proof_status {
                log::info!("DB: Updating proof status: {} -> {:?}", 
                          update_status.batch_id, update_status.new_status);

                if let Some(proof_record) = db.batch_proofs.get_mut(&update_status.batch_id) {
                    proof_record.status = update_status.new_status;
                    proof_record.updated_at = SystemTime::now();
                    proof_record.error_message = update_status.error_message;
                    
                    log::info!("Proof status updated successfully");
                } else {
                    log::error!("Batch proof not found: {}", update_status.batch_id);
                }
            }
            else if let Some(batch_id) = msg.get_proof_by_batch_id {
                log::info!("DB: Looking up proof: {}", batch_id);
                
                if let Some(proof_record) = db.batch_proofs.get(&batch_id) {
                    log::info!("Found proof: {} with status: {:?}", batch_id, proof_record.status);
                    // TODO: Send proof back through a response channel
                } else {
                    log::warn!("No proof found for batch_id: {}", batch_id);
                }
            }
            else if let Some(_get_unsettled) = msg.get_unsettled_proofs {
                let unsettled: Vec<&BatchProofRecord> = db.batch_proofs
                    .values()
                    .filter(|p| matches!(p.status, ProofStatus::Generated | ProofStatus::Posted | ProofStatus::Failed))
                    .collect();
                
                log::info!("DB: Found {} unsettled proofs", unsettled.len());
                for proof in &unsettled {
                    log::info!("  - {}: {:?} (retry: {})", 
                              proof.batch_id, proof.status, proof.retry_count);
                }
                // TODO: Send unsettled proofs and diagnostics back thruogh a response channel
            }
            
            else if let Some(_retry_failed) = msg.retry_failed_proofs {
                log::info!("DB: Manual retry triggered");
                
                let failed_proofs: Vec<(String, BatchProofRecord)> = db.batch_proofs
                    .iter()
                    .filter(|(_, proof)| proof.status == ProofStatus::Failed && proof.retry_count < 3)
                    .map(|(batch_id, proof)| (batch_id.clone(), proof.clone()))
                    .collect();

                log::info!("DB: Found {} failed proofs eligible for manual retry", failed_proofs.len());
                
                let mut success_count = 0;
                let mut fail_count = 0;
                
                for (batch_id, mut proof_record) in failed_proofs {
                    proof_record.retry_count += 1;
                    proof_record.status = ProofStatus::Generated;
                    proof_record.updated_at = SystemTime::now();
                    proof_record.error_message = Some(format!("Manual retry attempt #{}", proof_record.retry_count));
                    
                    db.batch_proofs.insert(batch_id.clone(), proof_record.clone());
                    
                    let retry_job = SettlementJob {
                        batch_id: batch_id.clone(),
                        proof_data: Some(proof_record.proof_data),
                        transaction_signatures: proof_record.transaction_signatures,
                        proof_file_path: Some(format!("build/proof_batch_{}.json", batch_id)),
                    };
                    
                    match settlement_sender.try_send(retry_job) {
                        Ok(()) => {
                            log::info!("  - Successfully queued manual retry: {}", batch_id);
                            success_count += 1;
                        }
                        Err(e) => {
                            log::error!("  - Failed to queue manual retry {}: {}", batch_id, e);
                            fail_count += 1;
                            
                            if let Some(proof) = db.batch_proofs.get_mut(&batch_id) {
                                proof.status = ProofStatus::Failed;
                                proof.error_message = Some(format!("Failed to queue retry: {}", e));
                            }
                        }
                    }
                }
                
                log::info!("Manual retry complete - Success: {}, Failed: {}", success_count, fail_count);
            }

            else if let Some(_trigger_retry) = msg.trigger_retry_cycle {
                log::info!("DB: Automatic retry cycle requested");
                
                // Circuit breaker check
                if !db.should_allow_retry_cycle() {
                    continue;
                }
                
                let failed_proofs: Vec<(String, BatchProofRecord)> = db.batch_proofs
                    .iter()
                    .filter(|(_, proof)| proof.status == ProofStatus::Failed && proof.retry_count < 3)
                    .map(|(batch_id, proof)| (batch_id.clone(), proof.clone()))
                    .collect();

                if failed_proofs.is_empty() {
                    log::info!("DB: No failed proofs found for retry cycle #{}", db.retry_cycle_count);
                    db.record_retry_cycle_result(0, 0);
                    continue;
                }

                log::info!("DB: Retry cycle #{} processing {} failed proofs", 
                          db.retry_cycle_count, failed_proofs.len());
                
                let mut success_count = 0;
                let mut fail_count = 0;
                let mut skip_count = 0;
                
                for (batch_id, mut proof_record) in failed_proofs {
                    // don't retry if already at max attempts
                    if proof_record.retry_count >= 3 {
                        log::debug!("  - Skipping {}: max retry attempts reached", batch_id);
                        skip_count += 1;
                        continue;
                    }
                    
                    // here we increment the retry count
                    proof_record.retry_count += 1;
                    proof_record.status = ProofStatus::Generated;
                    proof_record.updated_at = SystemTime::now();
                    proof_record.error_message = Some(format!("Auto-retry cycle #{}, attempt #{}", 
                                                             db.retry_cycle_count, proof_record.retry_count));
                    
                    db.batch_proofs.insert(batch_id.clone(), proof_record.clone());
                    
                    let retry_job = SettlementJob {
                        batch_id: batch_id.clone(),
                        proof_data: Some(proof_record.proof_data),
                        transaction_signatures: proof_record.transaction_signatures,
                        proof_file_path: Some(format!("build/proof_batch_{}.json", batch_id)),
                    };
                    
                    match settlement_sender.try_send(retry_job) {
                        Ok(()) => {
                            log::info!("  - Auto-retry queued: {} (attempt {})", batch_id, proof_record.retry_count);
                            success_count += 1;
                        }
                        Err(crossbeam::channel::TrySendError::Full(_)) => {
                            log::warn!("  - Settlement queue full for {}", batch_id);
                            fail_count += 1;
                            
                            if let Some(proof) = db.batch_proofs.get_mut(&batch_id) {
                                proof.status = ProofStatus::Failed;
                                proof.error_message = Some("Settlement queue full".to_string());
                            }
                        }
                        Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                            log::error!("  - Settlement channel disconnected for {}", batch_id);
                            fail_count += 1;
                            
                            if let Some(proof) = db.batch_proofs.get_mut(&batch_id) {
                                proof.status = ProofStatus::Failed;
                                proof.error_message = Some("Settlement channel disconnected".to_string());
                            }
                        }
                    }
                }
                
                // here we record results for our circuit breaker
                db.record_retry_cycle_result(success_count, fail_count);
                
                log::info!("DB: Retry cycle #{} complete - Success: {}, Failed: {}, Skipped: {}", 
                          db.retry_cycle_count, success_count, fail_count, skip_count);
                
                if fail_count > success_count && fail_count > 0 {
                    log::warn!("DB: Retry cycle #{} had more failures than successes - system may be degraded", 
                              db.retry_cycle_count);
                }
            }
            
            // this is deprecated
            else if let Some(settle_proof) = msg.add_settle_proof {
                log::warn!("DB: Received deprecated add_settle_proof: {} - use store_batch_proof instead", settle_proof);
            }
        }
        
        log::info!("RollupDB shutting down");
    }
}

impl ProofData {
    pub fn from_json_file(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file_content = std::fs::read_to_string(file_path)?;
        let json_value: serde_json::Value = serde_json::from_str(&file_content)?;
        
        Ok(ProofData {
            pi_a: [
                json_value["pi_a"][0].as_str().ok_or_else(|| format!("Missing or invalid pi_a[0] in {}", file_path))?.to_string(),
                json_value["pi_a"][1].as_str().ok_or_else(|| format!("Missing or invalid pi_a[1] in {}", file_path))?.to_string(),
                json_value["pi_a"][2].as_str().ok_or_else(|| format!("Missing or invalid pi_a[2] in {}", file_path))?.to_string(),
            ],
            pi_b: [
                [
                    json_value["pi_b"][0][0].as_str().ok_or_else(|| format!("Missing or invalid pi_b[0][0] in {}", file_path))?.to_string(),
                    json_value["pi_b"][0][1].as_str().ok_or_else(|| format!("Missing or invalid pi_b[0][1] in {}", file_path))?.to_string(),
                ],
                [
                    json_value["pi_b"][1][0].as_str().ok_or_else(|| format!("Missing or invalid pi_b[1][0] in {}", file_path))?.to_string(),
                    json_value["pi_b"][1][1].as_str().ok_or_else(|| format!("Missing or invalid pi_b[1][1] in {}", file_path))?.to_string(),
                ],
                [
                    json_value["pi_b"][2][0].as_str().ok_or_else(|| format!("Missing or invalid pi_b[2][0] in {}", file_path))?.to_string(),
                    json_value["pi_b"][2][1].as_str().ok_or_else(|| format!("Missing or invalid pi_b[2][1] in {}", file_path))?.to_string(),
                ],
            ],
            pi_c: [
                json_value["pi_c"][0].as_str().ok_or_else(|| format!("Missing or invalid pi_c[0] in {}", file_path))?.to_string(),
                json_value["pi_c"][1].as_str().ok_or_else(|| format!("Missing or invalid pi_c[1] in {}", file_path))?.to_string(),
                json_value["pi_c"][2].as_str().ok_or_else(|| format!("Missing or invalid pi_c[2] in {}", file_path))?.to_string(),
            ],
            protocol: json_value["protocol"].as_str().ok_or_else(|| format!("Missing or invalid protocol in {}", file_path))?.to_string(),
            curve: json_value["curve"].as_str().ok_or_else(|| format!("Missing or invalid curve in {}", file_path))?.to_string(),
        })
    }
}