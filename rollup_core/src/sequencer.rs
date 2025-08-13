use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use anyhow::{anyhow, Result};
use async_channel::Sender;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use solana_client::{nonblocking::rpc_client as nonblocking_rpc_client, rpc_client::RpcClient};
use solana_compute_budget::compute_budget::{
    ComputeBudget, SVMTransactionExecutionBudget, SVMTransactionExecutionCost,
};
use async_channel::Receiver;
use solana_program_runtime::{
    invoke_context::{self, EnvironmentConfig, InvokeContext},
    loaded_programs::{
        BlockRelation, ForkGraph, LoadProgramMetrics, ProgramCacheEntry, ProgramCacheForTxBatch,
        ProgramRuntimeEnvironments,
    },
    sysvar_cache,
};
use solana_svm_callback::InvokeContextCallback;
use solana_svm_feature_set::SVMFeatureSet;

use solana_bpf_loader_program::syscalls::create_program_runtime_environment_v1;
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount},
    clock::{Epoch, Slot},
    fee::FeeStructure,
    hash::{hash, Hash},
    pubkey::Pubkey,
    rent::Rent,
    rent_collector::RentCollector,
    transaction::{SanitizedTransaction, Transaction},
    transaction_context::TransactionContext,
};
use solana_svm::{
    transaction_processing_callback::TransactionProcessingCallback, transaction_processing_result::ProcessedTransaction, transaction_processor::{
        TransactionBatchProcessor, TransactionProcessingConfig, TransactionProcessingEnvironment,
    }
};

use crate::{
    processor::{
        create_transaction_batch_processor, get_transaction_check_results, RollupForkGraph,
    },
    rollupdb::RollupDBMessage,
    settle::settle_state,
    loader::RollupAccountLoader
};



fn process_transaction_batch(
    transaction_batch : &[Transaction],
    rollup_account_loader :&RollupAccountLoader,
    rollupdb_sender: &CBSender<RollupDBMessage>
)->Result<()>{
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

    let processing_config = TransactionProcessingConfig::default();

    let sanitized_txs: Vec<SanitizedTransaction> = transaction_batch
        .iter()
        .map(|tx| SanitizedTransaction::try_from_legacy_transaction(tx.clone(), &HashSet::new()).unwrap())
        .collect();

    println!("Loading and executing a batch of sanitized transactions...");
    let results = processor.load_and_execute_sanitized_transactions(
        rollup_account_loader,
        &sanitized_txs,
        get_transaction_check_results(sanitized_txs.len()),
        &processing_environment,
        &processing_config,
    );
    for (i, res) in results.processing_results.iter().enumerate() {
        let original_tx = &transaction_batch[i];

        match res {
            Ok(ProcessedTransaction::Executed(tx_details)) => {
                // This transaction was successful!
                let new_data = tx_details.loaded_transaction.accounts.clone();
                log::info!(
                    "✅ Transaction successful. Sending updated state to DB for tx: {:?}",
                    original_tx.signatures[0]
                );

                // Send the successful transaction and its new data to the database
                rollupdb_sender.send(RollupDBMessage {
                    lock_accounts: None,
                    add_processed_transaction: Some(original_tx.clone()),
                    add_new_data: Some(new_data),
                    frontend_get_tx: None,
                    add_settle_proof: None,
                })?;
            }
             Err(e) => {
                log::error!(
                    "❌ Transaction failed to execute for tx: {:?}, Error: {}",
                    original_tx.signatures[0],
                    e
                );
                // We do not send failed transactions to the DB to maintain state integrity.
            }
            _ => { // Catches other cases like FeesOnly
                 log::warn!(
                    "⚠️ Transaction produced no state change for tx: {:?}",
                    original_tx.signatures[0]
                );
            }
        }
    }
    // //extracting new account data from the results
    // let new_data: Vec<Option<Vec<(Pubkey,AccountSharedData)>>> = results.
    // processing_results
    // .iter()
    // .map(|res | {
    //     res.as_ref().ok().and_then(|processed_tx|{
    //         if let ProcessedTransaction::Executed(tx) = processed_tx{
    //             Some(tx.loaded_transaction.accounts.clone())
    //         }else {
    //             None
    //         }
    //     })
    // })
    // .collect();

    // //for testing
    // let first_index_data = new_data[0].as_ref().;
    // log::info!("Sequencer processed transaction, new data: {:?}", first_index_data);



    // // Lock accounts for the entire batch
    // // let accounts_to_lock: Vec<Pubkey> = transaction_batch
    // //     .iter()
    // //     .flat_map(|tx| tx.message.account_keys.clone())
    // //     .collect();

    // // Send the entire batch of processed transactions to RollupDB
    // for tx in transaction_batch {
    //     rollupdb_sender
    //         .send(RollupDBMessage {
    //             lock_accounts: None, //Locking will be handled diffrently by the DB
    //             frontend_get_tx: None,
    //             add_settle_proof: None,
    //             add_processed_transaction: Some(tx.clone()),
    //             add_new_data: Some(first_index_data.clone())
    //         })
    //         .map_err(|_| anyhow!("failed to send message to rollupdb"))?;
    // }

    Ok(())
}


pub async  fn run(
    sequencer_receiver_channel: CBReceiver<Transaction>,
    rollupdb_sender: CBSender<RollupDBMessage>,
    account_receiver : Receiver<Option<Vec<(Pubkey,AccountSharedData)>>>,
) -> Result<()> {
    let mut tx_counter = 0u32;

    let batch_size = 2;//adjust based on the requirements
    let mut transaction_batch : Vec<Transaction> = Vec::with_capacity(batch_size);
    let rpc_client_temp = RpcClient::new("https://api.devnet.solana.com".to_string());
    
    println!("Sequencer is running...");
    let mut rollup_account_loader = RollupAccountLoader::new(&rpc_client_temp);

    while let Ok(transaction) = sequencer_receiver_channel.recv() {
        println!("{}",transaction_batch.len());
        transaction_batch.push(transaction);

        if transaction_batch.len() >= batch_size {
            println!("processing a batch of {} transaction", transaction_batch.len());

            let accounts_to_lock: Vec<Pubkey> = transaction_batch
                .iter()
                .flat_map(|tx| tx.message.account_keys.clone())
                .collect::<HashSet<_>>() 
                .into_iter()
                .collect();

            rollupdb_sender.send(RollupDBMessage {
                lock_accounts: Some(accounts_to_lock),
                add_processed_transaction: None,
                add_new_data: None,
                frontend_get_tx: None,
                add_settle_proof: None,
            })?;
            
            if let Some(Some(accounts_data)) = account_receiver.recv().await.ok(){
                for (pubkey, account) in accounts_data {
                    rollup_account_loader.add_account(pubkey, account);
                }

                // the cache is pre-loaded and  process the entire batch
                if let Err(e) = process_transaction_batch(
                    &transaction_batch,
                    &rollup_account_loader,
                    &rollupdb_sender,
                ) {
                    log::error!("Error processing transaction batch: {}", e);
                }
            }else{
                log::error!("Sequencer: Failed to receive account data from DB. Skipping batch.");
            }

            tx_counter += transaction_batch.len() as u32;
            transaction_batch.clear();
            log::info!("Batch processing finished. Ready for new transactions.");
        }

            // // TODO Lock db to avoid state changes during settlement

            // // TODO Prepare root hash, or your own proof to send to chain
        if tx_counter >= 2 { // You might want to adjust this trigger
            log::info!("SETTLE to L1...");
            let message = b"my rollup state proof or commitment";
            let message_hash = hash(message);

            tokio::spawn(async move {
                match settle_state(message_hash.into()).await {
                    Ok(hash) => log::info!("Settle hash: {}", hash),
                    Err(e) => log::error!("Settle failed: {:?}", e),
                }
            });
            tx_counter = 0;
        }
    }
    Ok(())
}
