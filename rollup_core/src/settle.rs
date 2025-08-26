use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Signer,
    signer,
    transaction::Transaction,
};
use solana_system_interface::instruction as system_instruction;
use dotenvy::dotenv;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use std::{fs, time::Duration};
use tokio::time::sleep;

use crate::rollupdb::{RollupDBMessage, UpdateProofStatusMessage, ProofStatus, ProofData};

#[derive(Debug, Clone)]
pub struct SettlementJob {
    pub batch_id: String,
    pub proof_data: Option<ProofData>,
    pub transaction_signatures: Vec<String>,
    pub proof_file_path: Option<String>,
}

#[derive(Debug)]
pub enum SettlementResult {
    Success(String), 
    Failed(String),  
    Retry,       
}

pub async fn settle_batch_with_proof(
    settlement_job: SettlementJob,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<SettlementResult> {
    log::info!("Starting settlement for batch: {}", settlement_job.batch_id);
    
    // here we update proof status to 'posted'
    update_proof_status(
        &settlement_job.batch_id,
        ProofStatus::Posted,
        None,
        rollupdb_sender,
    )?;
    
    match settlement_job.proof_data.clone() {
        Some(proof_data) => {
            settle_with_proof(settlement_job, proof_data, rollupdb_sender).await
        }
        None => {
            log::warn!("No proof data provided for batch: {}, using fallback settlement", settlement_job.batch_id);
            settle_with_fallback_proof(settlement_job, rollupdb_sender).await
        }
    }
}

async fn settle_with_proof(
    settlement_job: SettlementJob,
    proof_data: ProofData,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<SettlementResult> {
    log::info!("Attempting proof settlement for batch: {}", settlement_job.batch_id);
    
    dotenv().ok();
    let rpc_client = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".into(),
        CommitmentConfig::confirmed(),
    );
    
    let path = std::env::var("KEYPAIR2")?;
    let payer = signer::keypair::read_keypair_file(path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;

    // TODO: This is wehre we can integrate with our proof verifier program and replace with simulation
    match simulate_verification(&proof_data).await {
        Ok(verification_result) => {
            if verification_result {
                log::info!(" Proof verification successful for batch: {}", settlement_job.batch_id);
                
                // TODO: Create settlement transction by calling the proof verifier program
                let settle_instruction = create_settlement_instruction(&payer.pubkey(), &proof_data)?;
                
                let recent_blockhash = rpc_client.get_latest_blockhash().await?;
                let transaction = Transaction::new_signed_with_payer(
                    &[settle_instruction],
                    Some(&payer.pubkey()),
                    &[&payer],
                    recent_blockhash,
                );
                
                match rpc_client.send_and_confirm_transaction(&transaction).await {
                    Ok(signature) => {
                        log::info!("Settlement transaction confirmed: {}", signature);
                        
                        // here we updte the proof status to 'verified'
                        update_proof_status(
                            &settlement_job.batch_id,
                            ProofStatus::Verified,
                            None,
                            rollupdb_sender,
                        )?;
                        
                        Ok(SettlementResult::Success(signature.to_string()))
                    }
                    Err(e) => {
                        log::error!(" Settlement transaction failed for batch {}: {}", settlement_job.batch_id, e);
                        
                        // here we update proof status to 'failed'
                        update_proof_status(
                            &settlement_job.batch_id,
                            ProofStatus::Failed,
                            Some(format!("Transaction failed: {}", e)),
                            rollupdb_sender,
                        )?;
                        
                        Ok(SettlementResult::Failed(e.to_string()))
                    }
                }
            } else {
                log::error!(" Proof verification failed for batch: {}", settlement_job.batch_id);
                
                update_proof_status(
                    &settlement_job.batch_id,
                    ProofStatus::Failed,
                    Some("Proof verification failed".to_string()),
                    rollupdb_sender,
                )?;
                
                Ok(SettlementResult::Failed("Proof verification failed".to_string()))
            }
        }
        Err(e) => {
            log::error!(" Error during proof verification for batch {}: {}", settlement_job.batch_id, e);
            
            update_proof_status(
                &settlement_job.batch_id,
                ProofStatus::Failed,
                Some(format!("Verification error: {}", e)),
                rollupdb_sender,
            )?;
            
            Ok(SettlementResult::Retry) 
        }
    }
}

async fn simulate_verification(proof_data: &ProofData) -> Result<bool> {
    log::info!("Simulating onchain proof verification...");
    
    sleep(Duration::from_millis(500)).await;
    
    // here we are checkng if the proof data looks valid
    let is_valid = !proof_data.pi_a[0].is_empty() 
                   && !proof_data.pi_b[0][0].is_empty() 
                   && !proof_data.pi_c[0].is_empty()
                   && proof_data.protocol == "groth16"
                   && proof_data.curve == "bn128";
    
    log::info!("Verification result: {}", if is_valid { "VALID" } else { "INVALID" });
    Ok(is_valid)
}

fn create_settlement_instruction(payer: &Pubkey, proof_data: &ProofData) -> Result<Instruction> {
    log::info!("Creating settlement instruction with proof data");
    
    // TODO: call proof verifier program
    let proof_hash = format!("proof_{}_{}", 
                            &proof_data.pi_a[0][..8], 
                            &proof_data.pi_c[0][..8]);
    
    // we are still using system transfer but with proof metadata
    Ok(system_instruction::transfer(
        payer,
        payer, 
        1, 
    ))
    
    // TODO: When we call our proof verifier program, we would replace the system transfer above with:
    // Ok(Instruction {
    //     program_id: PROOF_VERIFIER_PROGRAM_ID,
    //     accounts: vec![
    //         AccountMeta::new(*payer, true),
    //     ],
    //     data: serialize_proof_for_onchain_verification(proof_data)?,
    // })
}

// Settlement for when no proof data is available
async fn settle_with_fallback_proof(
    settlement_job: SettlementJob,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<SettlementResult> {
    log::warn!("Using settlement for batch: {}", settlement_job.batch_id);
    
    dotenv().ok();
    let rpc_client = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".into(),
        CommitmentConfig::confirmed(),
    );
    
    let path = std::env::var("KEYPAIR2")?;
    let payer = signer::keypair::read_keypair_file(path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;

    let settle_instruction = system_instruction::transfer(
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

    match rpc_client.send_and_confirm_transaction(&transaction).await {
        Ok(signature) => {
            log::info!(" Settlement completed: {}", signature);
            
            // here we update proof status to 'verified'
            update_proof_status(
                &settlement_job.batch_id,
                ProofStatus::Verified,
                Some("Fallback settlement".to_string()),
                rollupdb_sender,
            )?;
            
            Ok(SettlementResult::Success(signature.to_string()))
        }
        Err(e) => {
            log::error!(" Settlement failed: {}", e);
            
            update_proof_status(
                &settlement_job.batch_id,
                ProofStatus::Failed,
                Some(format!("Settlement failed: {}", e)),
                rollupdb_sender,
            )?;
            
            Ok(SettlementResult::Failed(e.to_string()))
        }
    }
}

fn update_proof_status(
    batch_id: &str,
    status: ProofStatus,
    error_message: Option<String>,
    rollupdb_sender: &CBSender<RollupDBMessage>,
) -> Result<()> {
    let update_message = UpdateProofStatusMessage {
        batch_id: batch_id.to_string(),
        new_status: status,
        error_message,
    };
    
    rollupdb_sender.send(RollupDBMessage {
        lock_accounts: None,
        add_processed_transaction: None,
        add_new_data: None,
        frontend_get_tx: None,
        add_settle_proof: None,
        store_batch_proof: None,
        update_proof_status: Some(update_message),
        get_proof_by_batch_id: None,
        get_unsettled_proofs: None,
        retry_failed_proofs: None,
        list_offset: None,        
        list_limit: None,
        trigger_retry_cycle: None,
    })?;
    
    Ok(())
}

// Settlement worker that processes settlement jobs
pub async fn run_settlement_worker(
    settlement_receiver: CBReceiver<SettlementJob>,
    rollupdb_sender: CBSender<RollupDBMessage>,
) -> Result<()> {
    log::info!("Settlement worker started");
    
    while let Ok(settlement_job) = settlement_receiver.recv() {
        log::info!("Received settlement job for batch: {}", settlement_job.batch_id);
        
        match settle_batch_with_proof(settlement_job.clone(), &rollupdb_sender).await {
            Ok(SettlementResult::Success(signature)) => {
                log::info!(" Settlement successful for batch {}: {}", settlement_job.batch_id, signature);
            }
            Ok(SettlementResult::Failed(error)) => {
                log::error!(" Settlement failed for batch {}: {}", settlement_job.batch_id, error);
            }
            Ok(SettlementResult::Retry) => {
                log::warn!("Settlement needs retry for batch: {}", settlement_job.batch_id);
                // TODO: Retry logic
            }
            Err(e) => {
                log::error!("Settlement error for batch {}: {}", settlement_job.batch_id, e);
            }
        }
    }
    
    Ok(())
}
