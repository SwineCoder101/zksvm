// src/rollupdb.rs
use async_channel::{Receiver as AReceiver, Sender as ASender};
use crossbeam::channel::Receiver as CBReceiver;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::AccountSharedData, keccak::Hash, pubkey::Pubkey, transaction::Transaction,
};
use std::collections::HashMap;

use crate::frontend::{FrontendMessage, TransactionWithHash};

pub struct RollupDBMessage {
    pub lock_accounts: Option<Vec<Pubkey>>,
    pub add_processed_transaction: Option<Transaction>,
    pub frontend_get_tx: Option<Hash>,
    pub list_offset: Option<u64>,
    pub list_limit: Option<u32>,
    pub add_settle_proof: Option<String>,
    pub add_new_data: Option<Vec<(Pubkey, AccountSharedData)>>,
}

#[derive(Debug, Default)]
pub struct RollupDB {
    accounts_db: HashMap<Pubkey, AccountSharedData>,
    locked_accounts: HashMap<Pubkey, AccountSharedData>,
    transactions: HashMap<Hash, Transaction>,
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
            // 1) Lock + fetch accounts (from cache or L1)
            if let Some(accounts_to_lock) = msg.lock_accounts {
                let mut fetched: Vec<(Pubkey, AccountSharedData)> = Vec::with_capacity(accounts_to_lock.len());
                for pubkey in accounts_to_lock {
                    let acc = db
                        .accounts_db
                        .remove(&pubkey)
                        .or_else(|| rpc_client.get_account(&pubkey).ok().map(Into::into));
                    match acc {
                        Some(data) => {
                            db.locked_accounts.insert(pubkey, data.clone());
                            fetched.push((pubkey, data));
                        }
                        None => {
                            log::error!("Could not load account {} from L1 or cache", pubkey);
                        }
                    }
                }
                if let Err(e) = account_sender.send(Some(fetched)).await {
                    log::error!("account_sender.send failed: {e}");
                }
                continue;
            }

            // 2) Processed tx + new state data => update DB, unlock accounts, index tx by keccak(sig0)
            if let (Some(tx), Some(new_data)) = (msg.add_processed_transaction, msg.add_new_data) {
                for (pubkey, data) in new_data {
                    db.accounts_db.insert(pubkey, data);
                }
                for pubkey in tx.message.account_keys.iter() {
                    db.locked_accounts.remove(pubkey);
                }
                // Your convention: keccak(hashv) of signature string bytes
                let tx_hash = solana_sdk::keccak::hashv(&[tx.signatures[0].to_string().as_bytes()]);
                db.transactions.insert(tx_hash, tx);
                log::info!("{:#?}", db.transactions);
                continue;
            }

            // 3) Single tx fetch
            if let Some(wanted) = msg.frontend_get_tx {
                if let Some(tx) = db.transactions.get(&wanted) {
                    let _ = frontend_sender
                        .send(FrontendMessage {
                            get_tx: Some(wanted),
                            transaction: Some(tx.clone()),
                            transactions: None,
                            total: None,
                            has_more: None,
                            error: None,
                        })
                        .await;
                } else {
                    let _ = frontend_sender
                        .send(FrontendMessage {
                            get_tx: Some(wanted),
                            transaction: None,
                            transactions: None,
                            total: None,
                            has_more: None,
                            error: Some("Transaction not found".to_string()),
                        })
                        .await;
                }
                continue;
            }

            // 4) List mode with pagination
            //    Deterministic order: sort by hash (desc). If you have a timestamp, sort by that instead.
            let mut keys: Vec<Hash> = db.transactions.keys().cloned().collect();
            keys.sort_by(|a, b| b.to_string().cmp(&a.to_string())); // descending

            let total = keys.len() as u64;
            let offset = msg.list_offset.unwrap_or(0);
            let limit = msg.list_limit.unwrap_or(50).clamp(1, 500) as usize;

            let start = offset.min(total) as usize;
            let end = (start + limit).min(total as usize);
            let window = &keys[start..end];

            let txs: Vec<TransactionWithHash> = window
                .iter()
                .filter_map(|h| db.transactions.get(h).map(|tx| (h, tx)))
                .map(|(h, tx)| TransactionWithHash {
                    hash: h.to_string(),
                    transaction: tx.clone(),
                })
                .collect();

            let has_more = (end as u64) < total;

            let _ = frontend_sender
                .send(FrontendMessage {
                    get_tx: None,
                    transaction: None,
                    transactions: Some(txs),
                    total: Some(total),
                    has_more: Some(has_more),
                    error: None,
                })
                .await;
        }
    }
}
