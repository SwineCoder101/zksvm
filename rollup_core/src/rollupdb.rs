use async_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    account::AccountSharedData, keccak::Hash, pubkey::Pubkey, transaction::Transaction,
};

use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use std::{
    collections::{HashMap, HashSet},
    default,
};
use solana_client::rpc_client::RpcClient;
use crate::frontend::FrontendMessage;

pub struct RollupDBMessage {
    pub lock_accounts: Option<Vec<Pubkey>>,
    pub add_processed_transaction: Option<Transaction>,
    pub frontend_get_tx: Option<Hash>,
    pub add_settle_proof: Option<String>,
    pub add_new_data: Option<Vec<(Pubkey,AccountSharedData)>>,
}

#[derive(Debug, Default,)]
pub struct RollupDB {
    accounts_db: HashMap<Pubkey, AccountSharedData>,
    locked_accounts: HashMap<Pubkey, AccountSharedData>,
    transactions: HashMap<Hash, Transaction>,
}

impl RollupDB {
    pub async fn run(
        rollup_db_receiver: CBReceiver<RollupDBMessage>,
        frontend_sender: Sender<FrontendMessage>,
        account_sender: Sender<Option<Vec<(Pubkey, AccountSharedData)>>>,
    ) {
        let mut db = RollupDB::default();

        while let Ok(message) = rollup_db_receiver.recv() {
            log::info!("RollupDB received a message");

            if let Some(accounts_to_lock) = message.lock_accounts {
                log::info!("DB: Received request to lock and fetch {} accounts.", accounts_to_lock.len());
                let mut fetched_accounts_data: Vec<(Pubkey, AccountSharedData)> = Vec::new();
                let rpc_client = RpcClient::new("https://api.devnet.solana.com".to_string());

                for pubkey in accounts_to_lock {
                    // Try to get from local cache first. If it's not there, fetch from L1.
                    let account_data = db.accounts_db.remove(&pubkey).or_else(|| {
                        log::warn!("Account {} not in local DB, fetching from L1.", pubkey);
                        rpc_client.get_account(&pubkey).ok().map(|acc| acc.into())
                    });

                    // If we successfully got the account data (from cache or L1), lock it.
                    if let Some(data) = account_data {
                        db.locked_accounts.insert(pubkey, data.clone());
                        fetched_accounts_data.push((pubkey, data));
                    } else {
                        log::error!("FATAL: Could not load account {} from L1.", pubkey);
                        // In a real system, you might need more robust error handling here.
                    }
                }
                // Send the locked account data back to the sequencer so it can process the transaction
                log::info!("DB: Sending {} accounts to sequencer.", fetched_accounts_data.len());
                account_sender.send(Some(fetched_accounts_data)).await.unwrap();
            
            } else if let (Some(tx), Some(new_data)) = (message.add_processed_transaction, message.add_new_data) {
                // This part is already correct in your code.
                log::info!("DB: Received processed transaction. Updating state.");

                for (pubkey, account_data) in new_data {
                    db.accounts_db.insert(pubkey, account_data);
                }
                for pubkey in tx.message.account_keys.iter() {
                    db.locked_accounts.remove(pubkey);
                }
                let tx_hash = solana_sdk::keccak::hashv(&[&tx.signatures[0].to_string().as_bytes()]);
                db.transactions.insert(tx_hash, tx);
                log::info!("State update complete. Locked: {}, Unlocked: {}.", db.locked_accounts.len(), db.accounts_db.len());

            } else if let Some(get_this_hash_tx) = message.frontend_get_tx {
                // This part is also correct.
                log::info!("Received request from frontend for tx hash: {}", get_this_hash_tx);
                if let Some(req_tx) = db.transactions.get(&get_this_hash_tx) {
                    log::info!("✅ Found transaction for hash: {}", get_this_hash_tx);
                    frontend_sender.send(FrontendMessage {
                        transaction: Some(req_tx.clone()),
                        get_tx: None,
                    }).await.unwrap();
                } else {
                    log::warn!("⚠️ No transaction found for hash: {}", get_this_hash_tx);
                }
            }
        }
    }
}