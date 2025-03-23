use crate::connection::ConnectionManager;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::address;
use alloy::rpc::types::{Filter, Header, Transaction};
use alloy::sol;
use eyre::Result;
use futures::future;
use futures_util::StreamExt;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{Instrument, debug, error, info, instrument, warn};

pub static EVENT_CHANNEL: LazyLock<(Mutex<mpsc::Sender<Events>>, Mutex<mpsc::Receiver<Events>>)> =
    LazyLock::new(|| {
        let (sender, receiver) = mpsc::channel(100);
        (Mutex::new(sender), Mutex::new(receiver))
    });

pub static COMPLETION_CHANNEL: LazyLock<(
    Mutex<mpsc::Sender<CompletionEvent>>,
    Mutex<mpsc::Receiver<CompletionEvent>>,
)> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel(100);
    (Mutex::new(sender), Mutex::new(receiver))
});

sol!(
    #[allow(missing_docs)]
    function transfer(address to, uint256 amount) external returns(bool);
    function swapExactTokensForTokens(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] calldata path,
        address to,
        uint256 deadline
    ) external returns (uint256[] memory amounts);
);

pub enum Events {
    NewBlock(Header),
    NewTransaction(Transaction),
    PendingTransaction(Transaction),
    DexEvent {
        address: String,
        topics: String,
        data: String,
    },
}
pub enum CompletionEvent {
    BlockProcessed {
        block_number: String,
        tx_count: usize,
    },
    TransactionProcessed {
        tx_hash: String,
    },
    PendingTransactionProcessed {
        tx_hash: String,
    },
    DexEventProcessed {
        address: String,
    },
    Error {
        context: String,
        message: String,
    },
}

pub struct EventMonitor {
    pub connection_manager: Arc<ConnectionManager>,
    pub subscription_handle: Vec<JoinHandle<()>>,
}

#[allow(dead_code)]
impl EventMonitor {
    #[instrument(name = "event_monitor_initialize")]
    pub async fn initialize() -> Result<Self> {
        info!("Initializing EventMonitor");
        Ok(Self {
            connection_manager: Arc::new(ConnectionManager::initialize().await?),
            subscription_handle: Vec::new(),
        })
    }
    pub async fn join_all_tasks(&mut self) {
        info!("joining all subscription tasks");
        let mut results = Vec::new();
        for handle in self.subscription_handle.drain(..) {
            results.push(handle);
        }
        for result in future::join_all(results).await {
            if let Err(e) = result {
                error!(error = %e, "Task failed with error");
            }
        }
        info!("All handles are joined!");
    }
    #[instrument(skip(self), name = "monitor_blocks")]
    pub async fn monitor_blocks(&mut self) -> Result<()> {
        info!("Starting block monitoring");
        let provider = self.connection_manager.wss_provider.clone();
        let subscription = provider.subscribe_blocks().await?;
        let mut stream = subscription.into_stream();

        let sender = &EVENT_CHANNEL.0;
        let handle = tokio::spawn(async move {
            info!("Block monitoring task started");
            while let Some(block) = stream.next().await {
                debug!(block_number = ?block.number, block_hash = ?block.hash, "Received new block");
                let sender = sender.lock().await;
                if let Err(e) = sender.send(Events::NewBlock(block)).await {
                    error!(error = %e, "Failed to send block event");
                    break;
                }
            }
        }.in_current_span());
        self.subscription_handle.push(handle);
        Ok(())
    }
    #[instrument(skip(self), name = "monitor_dex")]
    pub async fn monitor_dex(&mut self) -> Result<()> {
        info!("starting dex monitoring");
        let provider = self.connection_manager.wss_provider.clone();
        let uniswap_token_address = address!("dac17f958d2ee523a2206206994597c13d831ec7"); // usdt
        let filter = Filter::new()
            .address(uniswap_token_address)
            // By specifying an `event` or `event_signature` we listen for a specific event of the
            // contract. In this case the `Transfer(address,address,uint256)` event.
            .event("Transfer(address,address,uint256)")
            .from_block(BlockNumberOrTag::Latest);
        let sub = provider.subscribe_logs(&filter).await?;
        let mut stream = sub.into_stream();
        let sender = &EVENT_CHANNEL.0;

        let handle = tokio::spawn(
            async move {
                info!("Dex monitoring task started!");
                while let Some(log) = stream.next().await {
                    let address_str = format!("{:?}", log.address());
                    let topics_str = format!("{:?}", log.topics());
                    let data_str = format!("{:?}", log.data());

                    info!(
                        address = %address_str,
                        topics = %topics_str,
                        data = %data_str,
                        "Log received!"
                    );

                    let sender = sender.lock().await;
                    if let Err(e) = sender
                        .send(Events::DexEvent {
                            address: address_str,
                            topics: topics_str,
                            data: data_str,
                        })
                        .await
                    {
                        error!(error = %e, "Failed to send DEX event");
                        break;
                    }
                }
            }
            .in_current_span(),
        );
        self.subscription_handle.push(handle);
        Ok(())
    }
    #[instrument(skip(self), name = "monitor_pending_tx")]
    pub async fn monitor_pending_tx(&mut self) -> Result<()> {
        info!("Starting pending transaction monitoring");
        let provider = self.connection_manager.wss_provider.clone();
        let subscription = provider.subscribe_pending_transactions().await?;

        let mut stream = subscription.into_stream();
        let sender = &EVENT_CHANNEL.0;
        let https_provider = self.connection_manager.https_provider.clone();

        let handle = tokio::spawn(
            async move {
                info!("Pending transaction monitoring task started");

                while let Some(tx_hash) = stream.next().await {
                    info!(tx_hash = ?tx_hash, "Received pending transaction hash");

                    match https_provider.get_transaction_by_hash(tx_hash).await {
                        Ok(Some(tx)) => {
                            info!(tx_hash = ?tx_hash, "Successfully retrieved transaction details");

                            if let Some(block_hash) = tx.block_hash {
                                info!(block_hash = ?block_hash, "Block hash");
                            }
                            if let Some(block_number) = tx.block_number {
                                info!(block_number = ?block_number, "Block number");
                            }
                            if let Some(tx_index) = tx.transaction_index {
                                info!(tx_index = ?tx_index, "Transaction index");
                            }
                            if let Some(gas_price) = tx.effective_gas_price {
                                info!(gas_price = ?gas_price, "Effective gas price");
                            }
                            let tx_addr = tx.inner.clone().into_parts().1;
                            info!(tx_addr = ?tx_addr, "Transaction address part");

                            // Send the transaction event to the channel
                            let sender = sender.lock().await;
                            if let Err(e) = sender.send(Events::PendingTransaction(tx)).await {
                                error!(error = %e, "Failed to send pending transaction event");
                                break;
                            }
                        }
                        Ok(None) => {
                            warn!(tx_hash = ?tx_hash, "Transaction not found");
                        }
                        Err(e) => {
                            error!(tx_hash = ?tx_hash, error = %e, "Failed to get transaction");
                        }
                    }
                }
            }
            .in_current_span(),
        );

        self.subscription_handle.push(handle);
        Ok(())
    }
    #[instrument(skip(self, block), fields(block_number = ?block.number, block_hash = ?block.hash))]
    pub async fn process_block_tx(&mut self, block: Header) -> Result<()> {
        debug!("Processing transactions for block");

        let provider = self.connection_manager.https_provider.clone();
        let event_sender = EVENT_CHANNEL.0.lock().await;
        let completion_sender = COMPLETION_CHANNEL.0.lock().await;

        let block_hash = block.hash;
        let block_number = block.number;

        match provider.get_block_by_hash(block_hash).await {
            Ok(Some(block_with_tx)) => {
                if let Some(txs) = block_with_tx.transactions.as_transactions() {
                    info!(
                        block_number = ?block_number,
                        tx_count = txs.len(),
                        "Processing block transactions"
                    );

                    for tx in txs {
                        if let Err(e) = event_sender.send(Events::NewTransaction(tx.clone())).await
                        {
                            error!(error = %e, "Failed to send transaction event");
                            if let Err(e) = completion_sender
                                .send(CompletionEvent::Error {
                                    context: format!("Block {block_number:?}"),
                                    message: format!("Failed to send transaction event: {e}"),
                                })
                                .await
                            {
                                error!(error = %e, "Failed to send completion event");
                            }
                            break;
                        }
                    }

                    if let Err(e) = completion_sender
                        .send(CompletionEvent::BlockProcessed {
                            block_number: format!("{block_number:?}"),
                            tx_count: txs.len(),
                        })
                        .await
                    {
                        error!(error = %e, "Failed to send block completion event");
                    }
                }
            }
            Ok(None) => {
                warn!(block_hash = ?block_hash, "Block not found");
                if let Err(e) = completion_sender
                    .send(CompletionEvent::Error {
                        context: format!("Block {block_number:?}"),
                        message: "Block not found".to_string(),
                    })
                    .await
                {
                    error!(error = %e, "Failed to send completion event");
                }
            }
            Err(e) => {
                error!(block_hash = ?block_hash, error = %e, "Failed to get block");
                if let Err(e) = completion_sender
                    .send(CompletionEvent::Error {
                        context: format!("Block {block_number:?}"),
                        message: format!("Failed to get block: {e}"),
                    })
                    .await
                {
                    error!(error = %e, "Failed to send completion event");
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(event_monitor), name = "process_events")]
    pub async fn process_events(event_monitor: Arc<Mutex<Self>>) -> Result<()> {
        info!("=========== STARTING EVENT PROCESSING ===========");

        loop {
            tokio::select! {
                event = async {
                    let mut rx = EVENT_CHANNEL.1.lock().await;
                    rx.recv().await
                } => {
                    if let Some(event) = event {
                        info!("Received an event from event channel!");
                        match event {
                            Events::NewBlock(header) => {
                                info!(
                                    block_number = ?header.number,
                                    block_hash = ?header.hash,
                                    "New block received"
                                );

                                let mut monitor = event_monitor.lock().await;
                                if let Err(e) = monitor.process_block_tx(header).await {
                                    error!(error = %e, "Failed to process block transactions");
                                    let sender = COMPLETION_CHANNEL.0.lock().await;
                                    let _ = sender.send(CompletionEvent::Error {
                                        context: "Block Processing".to_string(),
                                        message: format!("Failed to process block: {e}"),
                                    }).await;
                                }
                            },
                            Events::PendingTransaction(tx) => {
                                let tx_hash = tx.block_hash.unwrap_or_default();
                                info!(
                                    tx_hash = ?tx_hash,
                                    from = ?tx.block_number,
                                    to = ?tx.transaction_index,
                                    value = ?tx.effective_gas_price.unwrap(),
                                    "Pending transaction received"
                                );

                                let sender = COMPLETION_CHANNEL.0.lock().await;
                                if let Err(e) = sender.send(CompletionEvent::PendingTransactionProcessed {
                                    tx_hash: format!("{tx_hash:?}"),
                                }).await {
                                    error!(error = %e, "Failed to send pending tx completion event");
                                }
                            },
                            Events::DexEvent { address, topics, data } => {
                                info!(
                                    address = %address,
                                    "DEX event received"
                                );

                                let sender = COMPLETION_CHANNEL.0.lock().await;
                                if let Err(e) = sender.send(CompletionEvent::DexEventProcessed {
                                    address,
                                }).await {
                                    error!(error = %e, "Failed to send DEX event completion");
                                }
                            },
                            Events::NewTransaction(tx) => {
                                let tx_hash = tx.block_hash.unwrap_or_default();
                                info!(
                                    tx_hash = ?tx_hash,
                                    block_number = ?tx.block_number,
                                    value = ?tx.effective_gas_price.unwrap(),
                                    "New transaction confirmed"
                                );

                                let sender = COMPLETION_CHANNEL.0.lock().await;
                                if let Err(e) = sender.send(CompletionEvent::TransactionProcessed {
                                    tx_hash: format!("{tx_hash:?}"),
                                }).await {
                                    error!(error = %e, "Failed to send tx completion event");
                                }
                            }
                        }
                    } else {
                        info!("Event channel closed, exiting event processing loop");
                        break;
                    }
                },

                completion = async {
                    let mut rx = COMPLETION_CHANNEL.1.lock().await;
                    rx.recv().await
                } => {
                    if let Some(completion) = completion {
                        match completion {
                            CompletionEvent::BlockProcessed { block_number, tx_count } => {
                                info!(
                                    block_number = %block_number,
                                    tx_count = tx_count,
                                    "Block processing completed"
                                );
                            },
                            CompletionEvent::TransactionProcessed { tx_hash } => {
                                debug!(
                                    tx_hash = %tx_hash,
                                    "Transaction processing completed"
                                );
                            },
                            CompletionEvent::PendingTransactionProcessed { tx_hash } => {
                                debug!(
                                    tx_hash = %tx_hash,
                                    "Pending transaction processing completed"
                                );
                            },
                            CompletionEvent::DexEventProcessed { address } => {
                                debug!(
                                    address = %address,
                                    "DEX event processing completed"
                                );
                            },
                            CompletionEvent::Error { context, message } => {
                                error!(
                                    context = %context,
                                    error = %message,
                                    "Error during event processing"
                                );
                            }
                        }
                    } else {
                        info!("Completion channel closed, exiting event processing loop");
                        break;
                    }
                },

                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                    debug!("No events received in the last 60 seconds, waiting...");
                    continue;
                }
            }
        }

        info!("Event processing loop completed");
        Ok(())
    }
    #[instrument(skip(self), name = "start_monitoring")]
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting blockchain monitoring services");

        self.monitor_blocks().await?;
        self.monitor_dex().await?;
        self.monitor_pending_tx().await?;

        info!("All monitoring services started successfully");
        Ok(())
    }
    #[instrument(name = "event_monitor_run")]
    pub async fn run() -> Result<()> {
        info!("Starting EventMonitor main loop");

        let monitor = Arc::new(Mutex::new(Self::initialize().await?));
        info!("EventMonitor initialized successfully");

        {
            let mut monitor_lock = monitor.lock().await;
            monitor_lock.start_monitoring().await?;
        }

        info!("=========== STARTING EVENT PROCESSING ===========");
        Self::process_events(monitor).await?;

        info!("EventMonitor main loop completed");
        Ok(())
    }
}
