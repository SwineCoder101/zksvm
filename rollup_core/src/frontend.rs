use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use actix_web::{error, web, HttpResponse};
use async_channel::Receiver;
use crossbeam::channel::{Receiver as CBReceiver, Sender as CBSender};
use serde::{Deserialize, Serialize};
use solana_sdk::keccak::Hash;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

use crate::rollupdb::RollupDBMessage;

// message format to send found transaction from db to frontend
pub struct FrontendMessage {
    pub get_tx: Option<Hash>,
    pub transaction: Option<Transaction>,
    pub error: Option<String>,
}

// message format used to get transaction client
#[derive(Serialize, Deserialize, Debug)]
pub struct GetTransaction {
    pub get_tx: String,
}

// unified message format for frontend <-> backend
#[derive(Serialize, Deserialize, Debug)]
pub struct RollupTransaction {
    pub sender: String,
    pub sol_transaction: Option<Transaction>,
    pub error: Option<String>,
}

pub async fn submit_transaction(
    body: web::Json<RollupTransaction>,
    sequencer_sender: web::Data<CBSender<Transaction>>,
) -> actix_web::Result<HttpResponse> {
    log::info!("Submitted transaction");
    log::info!("{body:?}");

    if let Some(tx) = body.sol_transaction.clone() {
        sequencer_sender.send(tx).unwrap();
        Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "Submitted")])))
    } else {
        log::warn!("Submit request missing sol_transaction");
        Ok(HttpResponse::BadRequest().json(HashMap::from([("error", "Missing sol_transaction")])))
    }
}

pub async fn get_transaction(
    body: web::Json<GetTransaction>,
    rollupdb_sender: web::Data<CBSender<RollupDBMessage>>,
    frontend_receiver: web::Data<Receiver<FrontendMessage>>,
) -> actix_web::Result<HttpResponse> {
    log::info!("Requested transaction: {:?}", body);
    let sig = body.get_tx.clone();

    rollupdb_sender
        .send(RollupDBMessage {
            lock_accounts: None,
            add_processed_transaction: None,
            frontend_get_tx: Some(
                Hash::from_str(&sig).map_err(|_| error::ErrorBadRequest("Invalid hash"))?,
            ),
            add_settle_proof: None,
            add_new_data: None,
        })
        .unwrap();

    if let Ok(frontend_message) = frontend_receiver.recv().await {
        if let Some(tx) = frontend_message.transaction {
            return Ok(HttpResponse::Ok().json(RollupTransaction {
                sender: "Rollup RPC".into(),
                sol_transaction: Some(tx),
                error: None,
            }));
        } else if let Some(err) = frontend_message.error {
            return Ok(HttpResponse::Ok().json(RollupTransaction {
                sender: "Rollup RPC".into(),
                sol_transaction: None,
                error: Some(err),
            }));
        }
    }

    Ok(HttpResponse::Ok().json(HashMap::from([("Transaction status", "requested")])))
}

pub async fn test() -> HttpResponse {
    log::info!("Test request");
    HttpResponse::Ok().json(HashMap::from([("test", "success")]))
}
