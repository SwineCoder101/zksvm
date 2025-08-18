
    use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use anyhow::{anyhow, Result};
use async_channel::Receiver;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use solana_client::rpc_client::RpcClient;
use solana_compute_budget::compute_budget::SVMTransactionExecutionBudget;
use solana_program_runtime::loaded_programs::ForkGraph;
use solana_sdk::{
    account::AccountSharedData,
    fee::FeeStructure,
    hash::{hash, Hash},
    pubkey::Pubkey,
    rent_collector::RentCollector,
    transaction::{SanitizedTransaction, Transaction},
};
use solana_svm::{
    transaction_processing_callback::TransactionProcessingCallback,
    transaction_processing_result::ProcessedTransaction,
    transaction_processor::{
        TransactionBatchProcessor, TransactionProcessingConfig, TransactionProcessingEnvironment,
    },
};
use solana_svm_feature_set::SVMFeatureSet;

use crate::{
    loader::RollupAccountLoader,
    processor::{create_transaction_batch_processor, get_transaction_check_results, RollupForkGraph},
    rollupdb::RollupDBMessage,
    settle::settle_state, SettlementJob,
};


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
                // Update the loader's cache with the new state for subsequent transactions in this same batch.
                for (pubkey, account_data) in &new_data {
                    rollup_account_loader.add_account(*pubkey, account_data.clone());
                }
                log::info!("✅ Transaction successful. Sending state update to DB for tx: {:?}", original_tx.signatures[0]);
                rollupdb_sender.send(RollupDBMessage {
                    lock_accounts: None,
                    add_processed_transaction: Some(original_tx.clone()),
                    add_new_data: Some(new_data),
                    frontend_get_tx: None,
                    add_settle_proof: None,
                    list_offset: None,
                    list_limit: None,
                })?;
            }
            Err(e) => {
                log::error!("❌ Transaction in batch failed: {:?}, Error: {}", original_tx.signatures[0], e);
                batch_failed = true;
            }
            _ => {
                log::warn!("⚠️ Transaction in batch had no effect: {:?}", original_tx.signatures[0]);
                batch_failed = true;
            }
        }
    }
    Ok(!batch_failed)
}

pub async fn run(
    sequencer_receiver_channel: CBReceiver<Transaction>,
    rollupdb_sender: CBSender<RollupDBMessage>,
    account_receiver: Receiver<Option<Vec<(Pubkey, AccountSharedData)>>>,
    settler_sender:CBSender<SettlementJob>
) -> Result<()> {
    let mut tx_counter = 0u32;
    let batch_size = 3;
    let mut transaction_batch: Vec<Transaction> = Vec::with_capacity(batch_size);
    let rpc_client_temp = RpcClient::new("https://api.devnet.solana.com".to_string());

    log::info!("Sequencer is running with a batch size of {}.", batch_size);
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
                list_offset: None,
                list_limit: None,
            })?;

            if let Some(Some(accounts_data)) = account_receiver.recv().await.ok() {
                if process_transaction_batch(
                    &transaction_batch,
                    &mut rollup_account_loader,
                    &rollupdb_sender,
                )? {
                    // Only increment the counter if the whole batch was successful
                    log::info!("tx counter{}",tx_counter);
                    tx_counter += transaction_batch.len() as u32;
                }

            } else {
                log::error!("Failed to receive account data from DB. Skipping batch.");
            }
            transaction_batch.clear();
            log::info!("Batch processing finished. Ready for new transactions.");
        }

        log::info!("{} tx counter ", tx_counter);

        if tx_counter >= 3 {
            log::info!("Settlement threshold reached. Settling to L1...");
            let message = b"my rollup state proof or commitment";
            let message_hash = hash(message);
            log::info!("inside this ");
            settler_sender.send(SettlementJob { proof_hash: message_hash })?;
            tx_counter = 0;
        }
    }
    Ok(())
}