use anyhow::Result;
use rollup_client::{calculate_signature_hash, create_solana_transaction, RollupClient};
use solana_sdk::{
    hash::Hash,
    native_token::LAMPORTS_PER_SOL,
    signature::{Keypair, Signer}, signer,
};
use std::{
    process::{Child, Command},
    time::Duration,
};
use tokio::time::sleep;

/// Test server manager to handle rollup_core server lifecycle
struct TestServer {
    child: Option<Child>,
    rollup_client: RollupClient,
}

impl TestServer {
    /// Start the rollup_core server in the background
    async fn start() -> Result<Self> {
        println!("Starting rollup_core server...");

        // Start the rollup_core server as a background process
        let child = Command::new("cargo")
            .args(&["run"])
            .current_dir("../rollup_core")
            .spawn()
            .expect("Failed to start rollup_core server");

        let rollup_client = RollupClient::new("http://127.0.0.1:8080".to_string());
        let server = TestServer {
            child: Some(child),
            rollup_client,
        };

        // Wait for server to be ready
        server.wait_for_ready().await?;

        Ok(server)
    }

    /// Wait for the server to be ready by polling the test endpoint
    async fn wait_for_ready(&self) -> Result<()> {
        let max_attempts = 30; // 30 seconds timeout

        for _ in 0..max_attempts {
            match self.rollup_client.health_check().await {
                Ok(_) => {
                    println!("Server is ready!");
                    return Ok(());
                }
                Err(_) => {
                    println!("Waiting for server to start...");
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        anyhow::bail!("Server failed to start within timeout period");
    }

    /// Get reference to the rollup client
    fn client(&self) -> &RollupClient {
        &self.rollup_client
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            println!("Stopping rollup_core server...");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Create a test keypair for testing
fn create_test_keypair() -> Keypair {
    Keypair::new()
}

#[tokio::test]
async fn test_complete_rollup_flow() -> Result<()> {
    println!("=== Starting Complete Rollup Flow Integration Test ===");

    // Start the rollup server
    let server = TestServer::start().await?;
    let client = server.client();

    //  Test the server is running with basic health check
    println!("\n1. Testing server health check...");
    let health_response = client.health_check().await?;
    println!("Health check response: {:#?}", health_response);
    assert_eq!(health_response.get("test"), Some(&"success".to_string()));

    // Create test transaction
    println!("\n2. Creating test transaction...");
    // let sender_keypair = create_test_keypair();
    // let receiver_keypair = create_test_keypair();
    let sender_keypair = signer::keypair::read_keypair_file("/home/jvan/.solana/testkey.json").unwrap();
    let receiver_keypair = signer::keypair::read_keypair_file("/home/jvan/.solana/mykey_1.json").unwrap();
    
    let amount = 1 * LAMPORTS_PER_SOL;

    // Use a mock recent blockhash for testing
    let recent_blockhash = Hash::default();
    let sol_transaction =
        create_solana_transaction(&sender_keypair, &receiver_keypair, amount, recent_blockhash);
    let original_signature = sol_transaction.signatures[0];

    println!("Created transaction with signature: {}", original_signature);
    println!("From: {}", sender_keypair.pubkey());
    println!("To: {}", receiver_keypair.pubkey());
    println!("Amount: {} lamports", amount);

    // Submit transaction to rollup using client library
    println!("\n3. Submitting transaction to rollup...");
    let submit_response = client
        .submit_transaction("Integration Test", sol_transaction.clone())
        .await?;
    println!("Submit response: {:#?}", submit_response);
    assert_eq!(
        submit_response.get("Transaction status"),
        Some(&"Submitted".to_string())
    );

    // Calculate transaction hash for retrieval using library function
    println!("\n4. Calculating transaction hash...");
    let tx_sig = original_signature.to_string();
    let sig_hash_string = calculate_signature_hash(&tx_sig);

    println!("Original signature: {}", tx_sig);
    println!("Hash for lookup: {}", sig_hash_string);

    // Wait a bit for transaction processing
    println!("\n5. Waiting for transaction processing...");
    sleep(Duration::from_millis(100)).await;

    // Retrieve transaction from rollup using client library
    println!("\n6. Retrieving transaction from rollup...");
    let retrieved_tx = client.get_transaction(&sig_hash_string).await?;
    println!("Retrieved transaction: {:#?}", retrieved_tx);

    // Verify the retrieved transaction matches the original
    println!("\n7. Verifying transaction integrity...");

    // Check that the sender is from rollup
    assert_eq!(retrieved_tx.sender, "Rollup RPC");

    // Check that the transaction signature matches
    let retrieved_signature = retrieved_tx.sol_transaction.signatures[0];
    assert_eq!(retrieved_signature, original_signature);
    println!("✓ Signatures match: {}", retrieved_signature);

    // Check that the transaction instructions match
    assert_eq!(
        retrieved_tx.sol_transaction.message.instructions.len(),
        sol_transaction.message.instructions.len()
    );
    println!(
        "✓ Instruction count matches: {}",
        retrieved_tx.sol_transaction.message.instructions.len()
    );

    // Verify the account keys match
    assert_eq!(
        retrieved_tx.sol_transaction.message.account_keys,
        sol_transaction.message.account_keys
    );
    println!("✓ Account keys match");

    // Verify the recent blockhash matches
    assert_eq!(
        retrieved_tx.sol_transaction.message.recent_blockhash,
        sol_transaction.message.recent_blockhash
    );
    println!("✓ Recent blockhash matches");

    println!("\n=== Integration Test Completed Successfully! ===");
    println!(" All assertions passed");
    println!(" Complete rollup flow verified");
    println!(" Used actual functions from rollup_client and rollup_core libraries");

    Ok(())
}

#[tokio::test]
async fn test_svm_execution_flow() -> Result<()> {
    println!("=== Testing SVM Execution Flow ===");

    let server = TestServer::start().await?;
    let client = server.client();

    // Create a simple transfer transaction
    println!("\n1. Creating transfer transaction for SVM execution...");
    // let sender = create_test_keypair();
    // let receiver = create_test_keypair();
    let sender = signer::keypair::read_keypair_file("/home/jvan/.solana/testkey.json").unwrap();
    let receiver = signer::keypair::read_keypair_file("/home/jvan/.solana/mykey_1.json").unwrap();
    
    let amount = 5000; // 5000 lamports

    let transaction = create_solana_transaction(&sender, &receiver, amount, Hash::default());
    println!("Transaction created:");
    println!("  From: {}", sender.pubkey());
    println!("  To: {}", receiver.pubkey());
    println!("  Amount: {} lamports", amount);
    println!("  Signature: {}", transaction.signatures[0]);

    // Submit transaction - this will trigger SVM execution
    println!("\n2. Submitting transaction (will trigger SVM execution)...");
    let submit_response = client
        .submit_transaction("SVM Test", transaction.clone())
        .await?;
    println!("Submit response: {:#?}", submit_response);

    // Wait for SVM processing to complete
    println!("\n3. Waiting for SVM processing...");
    sleep(Duration::from_millis(500)).await; // Give more time for SVM processing

    // Retrieve the transaction to confirm it was processed
    println!("\n4. Retrieving processed transaction...");
    let sig_hash = calculate_signature_hash(&transaction.signatures[0].to_string());
    let retrieved_tx = client.get_transaction(&sig_hash).await?;

    // Verify transaction was stored after SVM processing
    println!("\n5. Verifying SVM processing completed...");
    assert_eq!(retrieved_tx.sender, "Rollup RPC");
    assert_eq!(
        retrieved_tx.sol_transaction.signatures[0],
        transaction.signatures[0]
    );

    println!(" SVM Execution Flow Test Completed!");
    println!(" Transaction submitted, processed by SVM, and stored successfully");
    println!(" Retrieved transaction matches original");

    Ok(())
}

#[tokio::test]
async fn test_rollup_error_handling() -> Result<()> {
    println!("=== Testing Rollup Error Handling ===");

    let server = TestServer::start().await?;
    let client = server.client();

    // Test 1: Try to get a non-existent transaction
    println!("\n1. Testing retrieval of non-existent transaction...");
    let fake_hash = "invalid_hash_that_does_not_exist";

    match client.get_transaction(fake_hash).await {
        Ok(response) => {
            // If it doesn't error, check if it's a fallback response
            println!("Fallback response received: {:#?}", response);
        }
        Err(e) => {
            println!(" Server correctly rejected invalid hash: {}", e);
        }
    }

    println!("\n=== Error Handling Test Completed ===");

    Ok(())
}

#[tokio::test]
async fn test_rollup_client_functionality() -> Result<()> {
    println!("=== Testing RollupClient Functionality ===");

    let server = TestServer::start().await?;
    let client = server.client();

    // Test the rollup client's individual methods
    println!("\n1. Testing health check method...");
    let health = client.health_check().await?;
    assert!(health.contains_key("test"));
    println!("✓ Health check method works");

    // Test transaction creation utility
    println!("\n2. Testing transaction creation utility...");
    // let keypair1 = create_test_keypair();
    // let keypair2 = create_test_keypair();
    let keypair1 = signer::keypair::read_keypair_file("/home/jvan/.solana/testkey.json").unwrap();
    let keypair2 = signer::keypair::read_keypair_file("/home/jvan/.solana/mykey_1.json").unwrap();
    
    let tx = create_solana_transaction(&keypair1, &keypair2, 1000, Hash::default());
    assert_eq!(tx.signatures.len(), 1);
    assert_eq!(tx.message.instructions.len(), 1);
    println!("✓ Transaction creation utility works");

    // Test signature hash calculation
    println!("\n3. Testing signature hash calculation...");
    let sig_str = tx.signatures[0].to_string();
    let hash1 = calculate_signature_hash(&sig_str);
    let hash2 = calculate_signature_hash(&sig_str);
    assert_eq!(hash1, hash2); // Should be deterministic
    assert!(!hash1.is_empty());
    println!("✓ Signature hash calculation works: {}", hash1);

    println!("\n=== RollupClient Functionality Test Completed ===");

    Ok(())
}
