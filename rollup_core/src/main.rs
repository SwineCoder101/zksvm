use std::thread;

use actix_web::{web, App, HttpServer};
use actix_cors::Cors;
use async_channel;
use crossbeam;
use frontend::FrontendMessage;
use rollupdb::{RollupDB, RollupDBMessage};
use settle::SettlementJob;
use solana_sdk::{account::AccountSharedData, pubkey::Pubkey, transaction::Transaction};
use tokio::{time::{interval, Duration}, runtime::Builder, join, signal};
use tokio_util::sync::CancellationToken;
mod frontend;
mod processor;
mod rollupdb;
mod sequencer;
mod settle;
mod loader;

// #[actix_web::main]
fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));

    log::info!("starting HTTP server at http://localhost:8080");

    // Create a shared shutdown token for coordinated shutdown
    let shutdown_token = CancellationToken::new();

    let (sequencer_sender, sequencer_receiver) = crossbeam::channel::unbounded::<Transaction>();
    let (rollupdb_sender, rollupdb_receiver) = crossbeam::channel::unbounded::<RollupDBMessage>();
    pub type PubkeyAccountSharedData = Option<Vec<(Pubkey, AccountSharedData)>>;
    let (account_sender, account_receiver) = async_channel::unbounded::<PubkeyAccountSharedData>();
    // let (sequencer_sender, sequencer_receiver) = async_channel::bounded::<Transaction>(100); // Channel for communication between frontend and sequencer
    // let (rollupdb_sender, rollupdb_receiver) = async_channel::unbounded::<RollupDBMessage>(); // Channel for communication between sequencer and accountsdb
    let (frontend_sender, frontend_receiver) = async_channel::unbounded::<FrontendMessage>(); // Channel for communication between data availability layer and frontend
                                                                                              // std::thread::spawn(sequencer::run(sequencer_receiver, rollupdb_sender.clone()));
    let (settler_sender,settler_receiver) = crossbeam::channel::unbounded::<SettlementJob>();

    let db_sender_for_settlement = rollupdb_sender.clone(); 
    let shutdown_token_settlement = shutdown_token.clone();
    let settler_handle = thread::spawn(move || {
        log::info!("Settlement worker starting...");
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            tokio::select! {
                result = settle::run_settlement_worker(settler_receiver, db_sender_for_settlement) => {
                    if let Err(e) = result {
                        log::error!("Settlement worker error: {}", e);
                    }
                }
                _ = shutdown_token_settlement.cancelled() => {
                    log::info!("Settlement worker received shutdown signal");
                }
            }
        });
        
        log::info!("Settlement worker stopped");
    });

    // let rt = Builder::new()
    //     .threaded_scheduler()
    //     .enable_all()
    //     .build()
    //     .unwrap();
    let db_sender2 = rollupdb_sender.clone();
    let fe_2 = frontend_sender.clone();
    let acc_sender = account_sender.clone();
    let settler_sender_for_db = settler_sender.clone();
    let retry_db_sender = rollupdb_sender.clone();
    let shutdown_token_processing = shutdown_token.clone();
    let asdserver_thread = thread::spawn(move || {
        log::info!("thread starting...");
        let rt = Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .unwrap();

        rt.block_on(async {
            let seq_handle = tokio::spawn({
                let shutdown_token_seq = shutdown_token_processing.clone();
                async move {
                log::info!("Sequencer starting...");
                tokio::select! {
                    result = sequencer::run(sequencer_receiver, db_sender2, account_receiver, settler_sender) => {
                        if let Err(e) = result {
                            log::error!("Sequencer error: {}", e);
                        }
                    }
                    _ = shutdown_token_seq.cancelled() => {
                        log::info!("Sequencer received shutdown signal");
                    }
                }
                log::info!("Sequencer stopped");
                }
            });

            let db_handle = tokio::spawn({
                let shutdown_token_db = shutdown_token_processing.clone();
                async move {
                    log::info!("RollupDB starting...");
                    tokio::select! {
                        _ = RollupDB::run(
                            rollupdb_receiver, 
                            fe_2,
                            acc_sender,
                            settler_sender_for_db
                        ) => {
                            log::info!("RollupDB completed naturally");
                        }
                        _ = shutdown_token_db.cancelled() => {
                            log::info!("RollupDB received shutdown signal");
                        }
                    }
                    log::info!("RollupDB stopped.");
                }
            });
        // Automatic retry every 5 minutes 
        let retry_handle = tokio::spawn({
            let shutdown_token_retry = shutdown_token_processing.clone();
            async move {
                let mut retry_interval = interval(Duration::from_secs(300)); 
                log::info!("retry timer starting (5min intervals)");
                
                loop {
                    tokio::select! {
                        _ = retry_interval.tick() => {
                            log::debug!("triggering periodic retry check...");
                            
                            let retry_message = RollupDBMessage {
                                lock_accounts: None,
                                add_processed_transaction: None,
                                frontend_get_tx: None,
                                list_offset: None,
                                list_limit: None,
                                add_settle_proof: None,
                                add_new_data: None,
                                store_batch_proof: None,
                                update_proof_status: None,
                                get_proof_by_batch_id: None,
                                get_unsettled_proofs: None,
                                retry_failed_proofs: None,
                                trigger_retry_cycle: Some(true),
                            };
                            
                            if retry_db_sender.send(retry_message).is_err() {
                                log::info!("retry timer stopping | database channel closed");
                                break;
                            }
                        }
                        
                        // Coordinated shutdown using CancellationToken
                        _ = shutdown_token_retry.cancelled() => {
                            log::info!("retry timer received coordinated shutdown signal");
                            break;
                        }
                    }
                }
                
                log::info!("retry timer stopped");
            }
        });

            // Wait for all processing tasks to complete or shutdown signal
            tokio::select! {
                _ = shutdown_token_processing.cancelled() => {
                    log::info!("Processing thread received shutdown signal");
                }
                // If no shutdown signal, wait for all tasks to complete naturally
                else => {
                    let _ = join!(seq_handle, db_handle, retry_handle);
                    log::info!("All processing tasks completed");
                }
            }
            
            log::info!("thread stopped");
        });
    });
    // Create sequencer task
    // tokio::spawn(sequencer::run(sequencer_receiver, rollupdb_sender.clone()));
    // tokio::task::spawn_blocking(|| sequencer::run(sequencer_receiver, rollupdb_sender.clone()) ).await.unwrap();
    // tokio::task::block_in_place(|| sequencer::run(sequencer_receiver, rollupdb_sender.clone()) ).await.unwrap();

    // Create rollup db task (accounts + transactions)
    // tokio::spawn(RollupDB::run(rollupdb_receiver, frontend_sender.clone()));

    // let frontend_receiver_mutex = Arc::new(Mutex::new(frontend_receiver));

    // Spawn the Actix Web server in a separate thread
    let shutdown_token_server = shutdown_token.clone();
    let server_thread = thread::spawn(move || {
        // Create a separate Tokio runtime for Actix Web
        let rt2 = Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();

        // Create frontend server
        rt2.block_on(async {
            let server = HttpServer::new(move || {
                App::new()
                    .wrap(
                        Cors::default()
                            .allowed_origin("http://localhost:3000")
                            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
                            .allowed_headers(vec!["Content-Type", "Authorization"])
                            .max_age(3600)
                    )
                    .app_data(web::Data::new(sequencer_sender.clone()))
                    .app_data(web::Data::new(rollupdb_sender.clone()))
                    .app_data(web::Data::new(frontend_sender.clone()))
                    .app_data(web::Data::new(frontend_receiver.clone()))
                    .route("/", web::get().to(frontend::test))
                    .route("/get_transaction", web::post().to(frontend::get_transaction))
                    .route("/submit_transaction", web::post().to(frontend::submit_transaction))
                //  .service(
                //      web::resource("/submit_transaction")
                //          .route(web::post().to(frontend::submit_transaction)),
                // )
            })
            .worker_max_blocking_threads(2)
            .bind("127.0.0.1:8080")
            .unwrap()
            .run();
            
            tokio::select! {
                server_result = server => {
                    if let Err(e) = server_result {
                        log::error!("HTTP server error: {}", e);
                    }
                }
                _ = shutdown_token_server.cancelled() => {
                    log::info!("HTTP server received shutdown signal");
                }
            }
            // tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        });
    });

    // Wait for SIGINT and coordinate shutdown
    let rt_main = Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    rt_main.block_on(async {
        // Wait for Ctrl+C
        match signal::ctrl_c().await {
            Ok(()) => {
                log::info!("SIGINT received; starting shutdown...");
                shutdown_token.cancel(); // Signal all tasks to shutdown
            }
            Err(err) => {
                log::error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    });

    log::info!("Waiting for threads to shutdown...");

    // Spawn a timeout thread
    let timeout_handle = thread::spawn(|| {
        thread::sleep(Duration::from_secs(5));
        log::warn!("Shutdown timeout reached, forcing exit");
        std::process::exit(0);
    });

    if let Err(e) = server_thread.join() {
        log::error!("HTTP server thread panicked: {:?}", e);
    }

    if let Err(e) = asdserver_thread.join() {
        log::error!("asdserver thread panicked: {:?}", e);
    }

    if let Err(e) = settler_handle.join() {
        log::error!("settlement thread panicked: {:?}", e);
    }

   
    drop(timeout_handle);

    // rt.shutdown_timeout(std::time::Duration::from_secs(20));

    log::info!("All threads stopped. Exiting.");
    // Ok(())
}