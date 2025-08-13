use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    blake3::Hash,
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    signer,
    system_program,
    transaction::Transaction,
};
use std::str::FromStr;

// Settle the state on solana, called by sequencer
pub async fn settle_state(proof: Hash) -> Result<String> {
    let rpc_client = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".into(),
        CommitmentConfig::confirmed(),
    );

    // Load keypair from the specified path
    // let payer = signer::keypair::read_keypair_file("/home/dev/.solana/testkey.json")
    //     .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;

    let payer = signer::keypair::read_keypair_file("/home/dev/.solana/testkey.json")
        .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;

    // Create a dummy system transfer instruction (transfers 0 lamports to self)
    let settle_instruction = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &payer.pubkey(),
        0, // 0 lamports - just a dummy transaction
    );

    // Get recent blockhash
    let recent_blockhash = rpc_client.get_latest_blockhash().await?;

    // Create and sign transaction with the dummy instruction
    let transaction = Transaction::new_signed_with_payer(
        &[settle_instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Send transaction to contract on chain
    let settle_tx_signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .await?;

    Ok(settle_tx_signature.to_string())
}

// Alternative: For when you need a different approach without hardcoded paths
pub async fn settle_state_with_custom_keypair(proof: Hash, keypair_path: &str) -> Result<String> {
    let rpc_client = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".into(),
        CommitmentConfig::confirmed(),
    );

    let payer = signer::keypair::read_keypair_file(keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;

    // Create a dummy system transfer (0 lamports to self) 
    let settle_instruction = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &payer.pubkey(), 
        0,
    );

    let recent_blockhash = rpc_client.get_latest_blockhash().await?;

    let transaction = Transaction::new_signed_with_payer(
        &[settle_instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let settle_tx_signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .await?;

    Ok(settle_tx_signature.to_string())
}