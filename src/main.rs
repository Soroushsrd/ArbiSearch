mod config;
mod connection;
mod event_handler;
mod utils;

use event_handler::EventMonitor;
use eyre::Result;
use tracing::info;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

//static mut LOGGING_GUARD: Option<Box<dyn std::any::Any + Send + Sync>> = None;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;
    info!("Starting Ethereum blockchain monitor...");

    EventMonitor::run().await?;

    Ok(())
}

fn setup_logging() -> Result<()> {
    let file_appender = rolling::daily("logs", "eth-monitor.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let terminal_layer = fmt::layer().with_ansi(true).with_target(true);

    let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);

    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    tracing_subscriber::registry()
        .with(filter)
        .with(terminal_layer)
        .with(file_layer)
        .init();

    std::mem::forget(guard);
    //unsafe { LOGGING_GUARD = Some(Box::new(guard)) };

    Ok(())
}
