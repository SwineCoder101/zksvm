use std::fs;

use anyhow::Result;
use dotenvy::dotenv;
use rollup_client::{calculate_signature_hash, create_solana_transaction, RollupClient};
use serde::{Deserialize, Serialize};
use serde_json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    native_token::LAMPORTS_PER_SOL,
    signature::Keypair,
    signer::{self, Signer},
};
/// tokio::time imports were unused, so removed.

#[derive(Serialize, Deserialize, Default)]
struct StoredBalances {
    kp2: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    // Load keypairs from files
    let path1 = std::env::var("KEYPAIR1")?;
    let path2 = std::env::var("KEYPAIR2")?;
    let keypair =
        signer::keypair::read_keypair_file(path1).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let keypair2 =
        signer::keypair::read_keypair_file(path2).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let rpc_client = RpcClient::new("https://api.devnet.solana.com".into());

    // Get recent blockhash from Solana
    let recent_blockhash: Hash = rpc_client.get_latest_blockhash().await?;

    // Create transaction using the library function (keypair2 -> keypair)
    let tx = create_solana_transaction(&keypair2, &keypair, 100_000, recent_blockhash);

    // Create rollup client
    let rollup_client = RollupClient::new("http://127.0.0.1:8080".to_string());

    println!("starting test response...");
    let test_response = rollup_client.health_check().await?;
    println!("{test_response:#?}");

    println!("Submitting transaction...");
    let submit_response = rollup_client
        .submit_transaction(Some("Me"), tx.clone())
        .await?;
    println!("{submit_response:#?}");
    println!("TX: {:?}", tx);

    // Compute keccak(signature) string used by the server as the lookup key
    let tx_sig = tx.signatures[0].to_string();
    let sig_hash = calculate_signature_hash(&tx_sig);
    println!("Sig: {}", tx_sig);
    println!("Sig_hash: {sig_hash}");

    // Load old balance if file exists
    let mut stored: StoredBalances = fs::read_to_string("balances.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let balance_kp2 = rpc_client.get_balance(&keypair2.pubkey()).await?;
    println!("--- Current run ---");

    if let Some(old) = stored.kp2 {
        let diff = balance_kp2 as i64 - old as i64;
        println!(
            "KP2 balance: {} lamports (~{} SOL), fees used: {} lamports",
            balance_kp2,
            balance_kp2 as f64 / LAMPORTS_PER_SOL as f64,
            -diff // negate so reductions show as positive "fees"
        );
    } else {
        println!(
            "KP2 balance: {} lamports (~{} SOL)",
            balance_kp2,
            balance_kp2 as f64 / LAMPORTS_PER_SOL as f64
        );
    }

    // Save the new balance for next run
    stored.kp2 = Some(balance_kp2);
    fs::write("balances.json", serde_json::to_string_pretty(&stored)?)?;

    // ---- Fetch paginated list (page 1) ----
    println!("Getting transactions page 1...");
    let page1 = rollup_client.get_transactions_page(1, 50).await?;
    println!("{:#?}", &page1.transactions);

    // ---- Optionally fetch ALL pages (be careful on big datasets) ----
    // let all_txs = rollup_client.get_all_transactions_paged(100).await?;
    // println!("Fetched ALL transactions: {}", all_txs.len());

    // ---- Fetch single transaction by its signature keccak hash ----
    println!("Getting single transaction by hash...");
    let tx_resp = rollup_client
        .get_transaction("BSqfcnbXsX4tADvmaVSX4BKuStTDwSiQPZnqwQtQ93N7")
        .await?;
    println!("{tx_resp:#?}");

    Ok(())
}
