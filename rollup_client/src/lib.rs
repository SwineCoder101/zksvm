use anyhow::{Error, Result};
use reqwest::Client;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    native_token::LAMPORTS_PER_SOL,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use std::collections::HashMap;
use rollup_core::frontend::RollupTransaction;


/// Create a Solana transaction for testing/demonstration
pub fn create_solana_transaction(
    from: &Keypair,
    to: &Keypair,
    amount: u64,
    recent_blockhash: Hash,
) -> Transaction {
    let ix = system_instruction::transfer(&from.pubkey(), &to.pubkey(), amount);
    Transaction::new_signed_with_payer(&[ix], Some(&from.pubkey()), &[from], recent_blockhash)
}

/// Submit a transaction to the rollup server
pub async fn submit_transaction_to_rollup(
    client: &Client,
    base_url: &str,
    sender_name: &str,
    transaction: Transaction,
) -> Result<HashMap<String, String>> {
    let rollup_tx = RollupTransaction {
        sender: sender_name.to_string(),
        sol_transaction: Some(transaction),
        error: None
    };

    let response = client
        .post(&format!("{}/submit_transaction", base_url))
        .json(&rollup_tx)
        .send()
        .await?
        .json::<HashMap<String, String>>()
        .await?;

    Ok(response)
}

/// Calculate the keccak hash of a transaction signature for lookup
pub fn calculate_signature_hash(signature: &str) -> String {
    solana_sdk::keccak::hashv(&[signature.as_bytes()]).to_string()
}

/// Get a transaction from the rollup server using its signature hash
pub async fn get_transaction_from_rollup(
    client: &Client,
    base_url: &str,
    signature_hash: &str,
) -> Result<RollupTransaction> {
    let get_request = HashMap::from([("get_tx", signature_hash.to_string())]);

    let response = client
        .post(&format!("{}/get_transaction", base_url))
        .json(&get_request)
        .send()
        .await?
        .json::<RollupTransaction>()
        .await?;

    Ok(response)
}

/// Perform a health check on the rollup server
pub async fn health_check(client: &Client, base_url: &str) -> Result<HashMap<String, String>> {
    let response = client
        .get(&format!("{}/", base_url))
        .send()
        .await?
        .json::<HashMap<String, String>>()
        .await?;

    Ok(response)
}

/// Create a complete rollup client for interacting with the server
pub struct RollupClient {
    client: Client,
    base_url: String,
}

impl RollupClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn health_check(&self) -> Result<HashMap<String, String>> {
        health_check(&self.client, &self.base_url).await
    }

    pub async fn submit_transaction(
        &self,
        sender_name: &str,
        transaction: Transaction,
    ) -> Result<HashMap<String, String>> {
        submit_transaction_to_rollup(&self.client, &self.base_url, sender_name, transaction).await
    }

    pub async fn get_transaction(&self, signature_hash: &str) -> Result<RollupTransaction> {
        get_transaction_from_rollup(&self.client, &self.base_url, signature_hash).await
    }
}
