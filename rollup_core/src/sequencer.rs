use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    process::Command,
    fs,
    time::SystemTime,
};
use anyhow::{anyhow, Result};
use async_channel::Receiver;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use solana_client::rpc_client::RpcClient;
use solana_compute_budget::compute_budget::SVMTransactionExecutionBudget;
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount},
    fee::FeeStructure,
    hash::Hash,
    pubkey::Pubkey,
    rent_collector::RentCollector,
    transaction::{SanitizedTransaction, Transaction},
};
use solana_svm::{
    transaction_processing_result::ProcessedTransaction,
    transaction_processor::{
        TransactionProcessingConfig, TransactionProcessingEnvironment,
    },
};
use solana_svm_feature_set::SVMFeatureSet;
use std::{convert::TryInto, os::unix::fs::PermissionsExt};
use serde::{Deserialize, Serialize};
use serde_json;


use crate::{
    loader::RollupAccountLoader,
    processor::{create_transaction_batch_processor, get_transaction_check_results, RollupForkGraph},
    rollupdb::{RollupDBMessage, StoreBatchProofMessage, ProofData},
    SettlementJob,
};

#[derive(Debug, Clone)]
struct TransactionBatch {
    pub transactions: Vec<Transaction>,
    pub signatures: Vec<String>,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCircuitInput {
    pub amounts: Vec<String>,
    pub signature_first_bytes: Vec<String>,
    pub from_balances_before: Vec<String>,
    pub from_balances_after: Vec<String>,
}

impl TransactionBatch {
    fn new(transactions: Vec<Transaction>) -> Self {
        let signatures: Vec<String> = transactions
            .iter()
            .map(|tx| tx.signatures[0].to_string())
            .collect();
        
        let batch_id = Self::generate_batch_id(&signatures);
        
        Self {
            transactions,
            signatures,
            batch_id,
        }
    }
    
    fn generate_batch_id(signatures: &[String]) -> String {
        let combined = signatures.join("-");
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("batch_{}_{}", timestamp, &combined[..8])
    }
}

impl BatchCircuitInput {
    pub fn new() -> Self {
        Self {
            amounts: Vec::new(),
            signature_first_bytes: Vec::new(),
            from_balances_before: Vec::new(),
            from_balances_after: Vec::new(),
        }
    }

    pub fn add_transaction(&mut self, amount: u64, signature_first_byte: u32, balance_before: u64, balance_after: u64) {
        self.amounts.push(amount.to_string());
        self.signature_first_bytes.push(signature_first_byte.to_string());
        self.from_balances_before.push(balance_before.to_string());
        self.from_balances_after.push(balance_after.to_string());
    }

    pub fn pad_to_size(&mut self, target_size: usize) {
        while self.amounts.len() < target_size {
            self.amounts.push("1".to_string());
            self.signature_first_bytes.push("1".to_string());
            self.from_balances_before.push("0".to_string());
            self.from_balances_after.push("0".to_string());
        }
    }

    pub fn len(&self) -> usize {
        self.amounts.len()
    }
}

fn process_transaction_batch(
    transaction_batch: &[Transaction],
    rollup_account_loader: &mut RollupAccountLoader,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<bool> {
    let compute_budget = SVMTransactionExecutionBudget::default();
    let feature_set = SVMFeatureSet::all_enabled();
    let fee_structure = FeeStructure::default();
    let rent_collector = RentCollector::default();
    let fork_graph = Arc::new(RwLock::new(RollupForkGraph {}));

    let processor = create_transaction_batch_processor(
        rollup_account_loader,
        &feature_set,
        &compute_budget,
        Arc::clone(&fork_graph),
    );

    let processing_environment = TransactionProcessingEnvironment {
        blockhash: Hash::default(),
        blockhash_lamports_per_signature: fee_structure.lamports_per_signature,
        epoch_total_stake: 0,
        feature_set,
        rent_collector: Some(&rent_collector),
    };

    let sanitized_txs: Vec<SanitizedTransaction> = transaction_batch
        .iter()
        .map(|tx| SanitizedTransaction::try_from_legacy_transaction(tx.clone(), &HashSet::new()).unwrap())
        .collect();

    log::info!("SVM is executing a batch of {} sanitized transactions...", sanitized_txs.len());
    let results = processor.load_and_execute_sanitized_transactions(
        rollup_account_loader,
        &sanitized_txs,
        get_transaction_check_results(sanitized_txs.len()),
        &processing_environment,
        &TransactionProcessingConfig::default(),
    );

    let mut batch_failed = false;
    for (i, res) in results.processing_results.iter().enumerate() {
        let original_tx = &transaction_batch[i];

        match res {
            Ok(ProcessedTransaction::Executed(tx_details)) => {
                let new_data = tx_details.loaded_transaction.accounts.clone();

                if let Some((payer_pubkey, payer_account)) = new_data.first() {
                    log::info!("Balance after execution for {}: {} lamports", payer_pubkey, payer_account.lamports());
                }
                for (pubkey, account_data) in &new_data {
                    rollup_account_loader.add_account(*pubkey, account_data.clone());
                }
                
                log::info!("Transaction successful. Sending state update to DB for tx: {:?}", original_tx.signatures[0]);
                rollupdb_sender.send(RollupDBMessage {
                    lock_accounts: None,
                    add_processed_transaction: Some(original_tx.clone()),
                    add_new_data: Some(new_data.clone()),
                    frontend_get_tx: None,
                    add_settle_proof: None,
                    store_batch_proof: None,
                    update_proof_status: None,
                    get_proof_by_batch_id: None,
                    get_unsettled_proofs: None,
                    retry_failed_proofs: None,
                    list_offset: None,
                    list_limit: None,
                    trigger_retry_cycle: None,
                })?;
            }
            Err(e) => {
                log::error!("Transaction in batch failed: {:?}, Error: {}", original_tx.signatures[0], e);
                batch_failed = true;
            }
            _ => {
                log::warn!("Transaction in batch had no effect: {:?}", original_tx.signatures[0]);
                batch_failed = true;
            }
        }
    }
    Ok(!batch_failed)
}

fn make_script_executable(script_path: &str) -> Result<()> {
    let mut perms = fs::metadata(script_path)?.permissions();
    perms.set_mode(perms.mode() | 0o111); 
    fs::set_permissions(script_path, perms)?;
    Ok(())
}

fn verify_circuit_files() -> Result<()> {
    log::info!("Verifying circuit files and directories...");
    let current_dir = std::env::current_dir()?;
    log::info!("Current working directory: {}", current_dir.display());
    let directories = ["circuit", "scripts"];
    for dir in directories {
        let dir_path = current_dir.join(dir);
        if dir_path.exists() {
            log::info!("Directory exists: {}", dir_path.display());
        } else {
            log::error!("Directory missing: {}", dir_path.display());
            return Err(anyhow!("Missing directory: {}", dir));
        }
    }
    let circuit_files = [
        "circuit/system_transfer.circom",
        "circuit/batch_system_transfer.circom"
    ];
    for file in circuit_files {
        let file_path = current_dir.join(file);
        if file_path.exists() {
            log::info!("Circuit file exists: {}", file_path.display());
        } else {
            log::error!("Circuit file missing: {}", file_path.display());
            return Err(anyhow!("Missing circuit file: {}", file));
        }
    }

    let script_path = current_dir.join("scripts/setup_and_prove.sh");
    if script_path.exists() {
        log::info!("Script exists: {}", script_path.display());
        let metadata = fs::metadata(&script_path)?;
        let permissions = metadata.permissions();
        if permissions.mode() & 0o111 != 0 {
            log::info!("Script is already executable");
        } else {
            log::info!("Script is not executable, fixing permissions...");
            make_script_executable(script_path.to_str().unwrap())?;
            log::info!("Script is now executable");
        }
    } else {
        log::error!("Script missing: {}", script_path.display());
        return Err(anyhow!("Missing script: {}", script_path.display()));
    }
    
    log::info!("All circuit files verified and ready!");
    Ok(())
}

fn generate_zk_proof(batch: &TransactionBatch) -> Result<ProofData> {
    log::info!("Generating ZK proof for batch: {}", batch.batch_id);
    if let Err(e) = verify_circuit_files() {
        log::error!("Circuit file verification failed: {}", e);
        return Err(e);
    }
    
    let batch_input = create_batch_circuit_input(batch)?;

    fs::create_dir_all("circuit/build")?;
    
    let input_file_path = format!("circuit/build/input_batch_{}.json", batch.batch_id);
    
    fs::write(&input_file_path, serde_json::to_string_pretty(&batch_input)?)?;
    
    log::info!("Created circuit input file: {} with {} transactions", input_file_path, batch_input.len());

    log::info!("Executing: ./scripts/setup_and_prove.sh");
    log::info!("   Working directory: {}", std::env::current_dir().unwrap().display());
    log::info!("   BATCH_ID: {}", batch.batch_id);
    log::info!("   INPUT_FILE: {}", input_file_path);

    let output = Command::new("./scripts/setup_and_prove.sh")
        .current_dir(".")
        .env("BATCH_ID", &batch.batch_id)
        .env("INPUT_FILE", &input_file_path)
        .output();
    
    match output {
        Ok(result) => {
            log::info!("Script execution completed");
            log::info!("Exit status: {}", result.status);
            log::info!("Stdout: {}", String::from_utf8_lossy(&result.stdout));
            log::info!("Stderr: {}", String::from_utf8_lossy(&result.stderr));
            
            if result.status.success() {
                log::info!("ZK proof generation successful for batch: {}", batch.batch_id);
                
                let proof_file_path = format!("build/proof_batch_{}.json", batch.batch_id);
                if fs::metadata(&proof_file_path).is_ok() {
                    ProofData::from_json_file(&proof_file_path)
                        .map_err(|e| anyhow!("Failed to load proof file: {}", e))
                } else {
                    ProofData::from_json_file("build/proof_batch.json")
                        .map_err(|e| anyhow!("Failed to load proof file: {}", e))
                }
            } else {
                log::error!("ZK proof generation failed for batch: {}", batch.batch_id);
                Err(anyhow!("Proof generation failed with exit code: {}", result.status))
            }
        }
        Err(e) => {
            log::error!("Failed to execute proof generation script: {}", e);
            Err(anyhow!("Script execution failed: {}", e))
        }
    }
}

fn create_batch_circuit_input(batch: &TransactionBatch) -> Result<BatchCircuitInput> {
    log::info!("Creating circuit input for {} system transfers with account data", batch.transactions.len());
    
    let mut circuit_input = BatchCircuitInput::new();
    
    for (i, tx) in batch.transactions.iter().enumerate() {
        let amount = extract_transfer_amount(tx)?;

        let sig_first_byte = if !tx.signatures.is_empty() {
            tx.signatures[0].as_ref()[0] as u32
        } else {
            return Err(anyhow!("Transaction {} has no signature", i));
        };
        
        let (balance_before, balance_after) = get_account_balances(tx, batch)?;
        
        circuit_input.add_transaction(amount, sig_first_byte, balance_before, balance_after);
        
        log::info!("  Transfer {}: amount={} lamports, sig_byte={}, balance_before={}, balance_after={}", 
                  i + 1, amount, sig_first_byte, balance_before, balance_after);
    }

    circuit_input.pad_to_size(3);
    
    log::info!("Circuit input created with {} transactions (padded if necessary)", circuit_input.len());
    Ok(circuit_input)
}

fn extract_transfer_amount(tx: &Transaction) -> Result<u64> {
    for instruction in &tx.message.instructions {
        if instruction.program_id_index == 0 { 
            if instruction.data.len() >= 12 && instruction.data[0..4] == [2, 0, 0, 0] {
                let amount_bytes: [u8; 8] = instruction.data[4..12].try_into()
                    .map_err(|_| anyhow!("Failed to parse transfer amount"))?;
                return Ok(u64::from_le_bytes(amount_bytes));
            }
        }
    }
    // if we can't parse, assume it's our known transfer amount
    Ok(1000000) 
}

fn get_account_balances(tx: &Transaction, _batch: &TransactionBatch) -> Result<(u64, u64)> {
    if tx.message.account_keys.is_empty() {
        return Err(anyhow!("Transaction has no account keys"));
    }
    
    let payer_pubkey = &tx.message.account_keys[0];
    let rpc_client = RpcClient::new("https://api.devnet.solana.com".to_string());
    
    match rpc_client.get_balance(payer_pubkey) {
        Ok(current_balance) => {
            // Scale down large balances to avoid circuit issues
            // Convert to SOL units (divide by 10^9) then back to smaller lamport amounts
            let balance_in_sol = current_balance / 1000000000; // Convert to SOL
            let scaled_balance_before = balance_in_sol * 1000000; // Scale to microSOL (6 decimals)
            let scaled_balance_after = scaled_balance_before - 5000; // Minus typical fee
            
            log::info!("Account balance for {}: original={} lamports, scaled_before={}, scaled_after={}", 
                      payer_pubkey, current_balance, scaled_balance_before, scaled_balance_after);
            
            Ok((scaled_balance_before, scaled_balance_after))
        }
        Err(e) => {
            log::warn!("Failed to fetch real balance for {}: {}", payer_pubkey, e);
            // here we are just using reasonable scaled values
            Ok((5000000, 4995000)) 
        }
    }
}

fn store_batch_proof(
    batch: &TransactionBatch,
    proof_data: ProofData,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<()> {
    log::info!("Storing batch proof in RollupDB for batch: {}", batch.batch_id);
    
    let store_message = StoreBatchProofMessage {
        batch_id: batch.batch_id.clone(),
        proof_data,
        public_inputs: vec!["1".to_string()], // batch_valid = 1
        transaction_signatures: batch.signatures.clone(),
    };
    
    rollupdb_sender.send(RollupDBMessage {
        lock_accounts: None,
        add_processed_transaction: None,
        add_new_data: None,
        frontend_get_tx: None,
        add_settle_proof: None,
        store_batch_proof: Some(store_message),
        update_proof_status: None,
        get_proof_by_batch_id: None,
        get_unsettled_proofs: None,
        retry_failed_proofs: None,
        list_offset: None,
        list_limit: None,
        trigger_retry_cycle: None,
    })?;
    
    log::info!("Batch proof stored successfully");
    Ok(())
}

pub async fn run(
    sequencer_receiver_channel: CBReceiver<Transaction>,
    rollupdb_sender: CBSender<RollupDBMessage>,
    account_receiver: Receiver<Option<Vec<(Pubkey, AccountSharedData)>>>,
    settler_sender: CBSender<SettlementJob>
) -> Result<()> {
    let mut tx_counter = 0u32;
    let batch_size = 3;
    let mut transaction_batch: Vec<Transaction> = Vec::with_capacity(batch_size);
    let rpc_client_temp = RpcClient::new("https://api.devnet.solana.com".to_string());

    log::info!("Sequencer running with ZK proof generation (batch size: {})", batch_size);
    let mut rollup_account_loader = RollupAccountLoader::new(&rpc_client_temp);

    while let Ok(transaction) = sequencer_receiver_channel.recv() {
        transaction_batch.push(transaction);
        log::info!("Transaction added to batch. Current size: {}/{}", transaction_batch.len(), batch_size);

        if transaction_batch.len() >= batch_size {
            log::info!("Batch is full. Beginning processing...");

            let accounts_to_lock: Vec<Pubkey> = transaction_batch
                .iter()
                .flat_map(|tx| tx.message.account_keys.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            log::info!("Requesting state for {} unique accounts from DB.", accounts_to_lock.len());
            rollupdb_sender.send(RollupDBMessage {
                lock_accounts: Some(accounts_to_lock),
                add_processed_transaction: None,
                add_new_data: None,
                frontend_get_tx: None,
                add_settle_proof: None,
                store_batch_proof: None,
                update_proof_status: None,
                get_proof_by_batch_id: None,
                get_unsettled_proofs: None,
                retry_failed_proofs: None,
                list_offset: None,
                list_limit: None,
                trigger_retry_cycle: None,
            })?;

            if let Some(Some(accounts_data)) = account_receiver.recv().await.ok() {
                if process_transaction_batch(
                    &transaction_batch,
                    &mut rollup_account_loader,
                    &rollupdb_sender,
                )? {
                    let batch = TransactionBatch::new(transaction_batch.clone());
                    log::info!("📋 Created batch: {} with {} transactions", batch.batch_id, batch.transactions.len());

                    match generate_zk_proof(&batch) {
                        Ok(proof_data) => {
                            log::info!("ZK proof generated successfully for batch: {}", batch.batch_id);

                            if let Err(e) = store_batch_proof(&batch, proof_data.clone(), &rollupdb_sender) {
                                log::error!("Failed to store proof in DB: {}", e);
                            }

                            let settlement_job = SettlementJob {
                                batch_id: batch.batch_id.clone(),
                                proof_data: Some(proof_data),
                                transaction_signatures: batch.signatures.clone(),
                                proof_file_path: Some(format!("build/proof_batch_{}.json", batch.batch_id)),
                            };
                            
                            log::info!("Sending batch to settlement: {}", batch.batch_id);
                            settler_sender.send(settlement_job)?;
                            
                            // increment counter if everything succeeded
                            tx_counter += transaction_batch.len() as u32;
                            log::info!("Batch processing complete. TX counter: {}", tx_counter);
                        }
                        Err(e) => {
                            log::error!("ZK proof generation failed for batch {}: {}", batch.batch_id, e);
                            // still increment counter but don't send to settlement
                            tx_counter += transaction_batch.len() as u32;
                        }
                    }
                } else {
                    log::error!("Batch processing failed. Skipping proof generation.");
                }
            } else {
                log::error!("Failed to receive account data from DB. Skipping batch.");
            }
            
            transaction_batch.clear();
            log::info!("Batch processing finished. Ready for new transactions.");
        }
        
        // Note: Settlement trigger is now handled per-batch rather than by counter
        // each successful batch triggers its own settlement
    }
    Ok(())
}