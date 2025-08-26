use anyhow::{anyhow, Result};
use reqwest::Client;
use std::collections::HashMap;
use solana_system_interface::instruction as system_instruction;
use rollup_core::frontend::{RollupTransaction, TransactionWithHash};
use solana_sdk::{
    hash::Hash,
    keccak,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

/// List response (matches server's paginated JSON)
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RollupTransactionsList {
    pub sender: Option<String>,
    pub transactions: Vec<TransactionWithHash>,
    pub page: u32,
    pub per_page: u32,
    pub total: Option<u64>,
    pub has_more: bool,
    pub error: Option<String>,
}

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
    sender_name: Option<&str>,
    transaction: Transaction,
) -> Result<HashMap<String, String>> {
    let rollup_tx = RollupTransaction {
        sender: sender_name.map(|s| s.to_string()),
        sol_transaction: Some(transaction),
        error: None,
    };

    let response = client
        .post(&format!("{}/submit_transaction", base_url.trim_end_matches('/')))
        .json(&rollup_tx)
        .send()
        .await?
        .error_for_status()? // surface non-2xx as Err
        .json::<HashMap<String, String>>()
        .await?;

    Ok(response)
}

/// Calculate the keccak hash of a transaction signature for lookup (string form)
pub fn calculate_signature_hash(signature: &str) -> String {
    keccak::hashv(&[signature.as_bytes()]).to_string()
}

/// Get a single transaction from the rollup server using its signature hash
pub async fn get_transaction_from_rollup(
    client: &Client,
    base_url: &str,
    signature_hash: &str,
) -> Result<RollupTransaction> {
    // server expects: { "get_tx": "<hash>" }
    let get_request = serde_json::json!({ "get_tx": signature_hash });

    let resp = client
        .post(&format!("{}/get_transaction", base_url.trim_end_matches('/')))
        .json(&get_request)
        .send()
        .await?
        .error_for_status()?
        .json::<RollupTransaction>()
        .await?;

    Ok(resp)
}

/// Get one page of transactions from the rollup server (paginated)
pub async fn get_transactions_page_from_rollup(
    client: &Client,
    base_url: &str,
    page: u32,
    per_page: u32,
) -> Result<RollupTransactionsList> {
    // server expects: { "page": <u32>, "per_page": <u32> } with no get_tx
    let get_request = serde_json::json!({ "page": page, "per_page": per_page });

    let resp = client
        .post(&format!("{}/get_transaction", base_url.trim_end_matches('/')))
        .json(&get_request)
        .send()
        .await?
        .error_for_status()?
        .json::<RollupTransactionsList>()
        .await?;

    Ok(resp)
}

/// Simple rollup client wrapper
pub struct RollupClient {
    client: Client,
    base_url: String,
}

impl RollupClient {
    pub fn new(base_url: String) -> Self {
        Self { client: Client::new(), base_url }
    }

pub async fn health_check(&self) -> Result<HashMap<String, String>> {
    let url = format!("{}/", self.base_url.trim_end_matches('/'));
    let resp = self.client.get(&url).send().await?.error_for_status()?;
    let map = resp.json::<HashMap<String, String>>().await?;
    Ok(map)
}



    pub async fn submit_transaction(
        &self,
        sender_name: Option<&str>,
        transaction: Transaction,
    ) -> Result<HashMap<String, String>> {
        submit_transaction_to_rollup(&self.client, &self.base_url, sender_name, transaction).await
    }

    /// Fetch a single tx by its signature-hash
    pub async fn get_transaction(&self, signature_hash: &str) -> Result<RollupTransaction> {
        get_transaction_from_rollup(&self.client, &self.base_url, signature_hash).await
    }

    /// Fetch one page (paginated)
    pub async fn get_transactions_page(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<RollupTransactionsList> {
        get_transactions_page_from_rollup(&self.client, &self.base_url, page, per_page).await
    }

    /// Convenience: fetch **all pages** (beware of large datasets)
    pub async fn get_all_transactions_paged(&self, per_page: u32) -> Result<Vec<TransactionWithHash>> {
        let per_page = per_page.clamp(1, 500);
        let mut page = 1;
        let mut out = Vec::new();

        loop {
            let resp = self.get_transactions_page(page, per_page).await?;
            if let Some(err) = &resp.error {
                // Early return on backend error
                return Err(anyhow!("Backend error on page {}: {}", page, err));
            }

            out.extend(resp.transactions.into_iter());

            if !resp.has_more {
                break;
            }
            page += 1;
        }

        Ok(out)
    }
}
