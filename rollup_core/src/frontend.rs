use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use actix_web::{error, web, HttpResponse};
use async_channel::{Receiver, Send, Sender};
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use serde::{Deserialize, Serialize};
use solana_sdk::keccak::{hashv, Hash};
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

use crate::rollupdb::RollupDBMessage;

// message format to send found transaction from db to frontend

pub struct FrontendMessage {
    pub get_tx: Option<Hash>,
    pub transaction: Option<Transaction>,
}

// message format used to get transaction client
#[derive(Serialize, Deserialize, Debug)]
pub struct GetTransaction {
    pub get_tx: String,
}

// message format used to receive transactions from clients
#[derive(Serialize, Deserialize, Debug)]
pub struct RollupTransaction {
    pub sender: String,
    pub sol_transaction: Transaction,
}

pub async fn submit_transaction(
    body: web::Json<RollupTransaction>,
    sequencer_sender: web::Data<CBSender<Transaction>>,
    // rollupdb_sender: web::Data<Sender<RollupDBMessage>>,
) -> actix_web::Result<HttpResponse> {
    // Validate transaction structure with serialization in function signature
    log::info!("Submitted transaction");
    log::info!("{body:?}");

    // Send transaction to sequencer
    sequencer_sender.send(body.sol_transaction.clone()).unwrap();

    // Return response
    Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "Submitted")])))
}

pub async fn get_transaction(
    body: web::Json<GetTransaction>,
    rollupdb_sender: web::Data<CBSender<RollupDBMessage>>,
    frontend_receiver: web::Data<Receiver<FrontendMessage>>,
) -> actix_web::Result<HttpResponse> {
    // Validate transaction structure with serialization in function signature
    log::info!("Requested transaction");
    log::info!("{body:?}");
    log::info!("ADD ROLLUPDB tx");
    let sig = body.get_tx.clone();
    log::info!("FRONTEND TX: {}", sig);
  

    rollupdb_sender
        .send(RollupDBMessage {
            lock_accounts: None,
            add_processed_transaction: None,
            frontend_get_tx: Some(
                Hash::from_str(&body.get_tx).map_err(|_| error::ErrorBadRequest("Invalid hash"))?,
            ),
            add_settle_proof: None,
            add_new_data:None
        })
        .unwrap();
    log::info!("Sending to rollupdb worked getrequest");

    if let Ok(frontend_message) = frontend_receiver.recv().await {
        log::info!("Rollup db got getrequest");
        return Ok(HttpResponse::Ok().json(RollupTransaction {
            sender: "Rollup RPC".into(),
            sol_transaction: frontend_message.transaction.unwrap(),
        }));
        // Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "requested")])))
    }

    Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "requested")])))
}

pub async fn test() -> HttpResponse {
    log::info!("Test request");
    HttpResponse::Ok().json(HashMap::from([("test", "success")]))
}
