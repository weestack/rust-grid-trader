mod algorithm;

use barter::{
    EngineEvent,
    engine::{
        clock::LiveClock,
        state::{
            global::DefaultGlobalData,
            instrument::{
                filter::InstrumentFilter,
            },
            trading::TradingState,
        },
    },
    logging::init_logging,
    risk::DefaultRiskManager,
    statistic::time::Daily,
    system::{
        builder::{AuditMode, EngineFeedMode, SystemArgs, SystemBuilder},
        config::SystemConfig,
    },
};
use barter_data::{
    streams::builder::dynamic::indexed::init_indexed_multi_exchange_market_stream,
    subscription::SubKind,
};
use barter_instrument::index::IndexedInstruments;
use barter_integration::Terminal;
use futures::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::{fs::File, io::BufReader, time::Duration};
use tracing::debug;
use crate::algorithm::data::AlgorithmData;
use crate::algorithm::grid::Grid;
use crate::algorithm::vwap::Vwap;

const FILE_PATH_SYSTEM_CONFIG: &str = "config/system_config.json";
const RISK_FREE_RETURN: Decimal = dec!(0.05);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialise Tracing
    init_logging();

    let config = load_config()?;

    // Extract USDT wallet size from config BEFORE destructuring
    let usdt_wallet_size = extract_usdt_wallet_size(&config);
    println!("📊 USDT Wallet Size: ${:.2}", usdt_wallet_size);

    // Now destructure the config
    let SystemConfig {
        instruments,
        executions,
    } = config;

    // Construct IndexedInstruments
    let instruments = IndexedInstruments::new(instruments);

    // Initialise MarketData Stream
    let market_stream = init_indexed_multi_exchange_market_stream(
        &instruments,
        &[SubKind::PublicTrades, SubKind::OrderBooksL2],
    )
        .await?;

    // Construct System Args with dynamic wallet size
    let args = SystemArgs::new(
        &instruments,
        executions,
        LiveClock,
        Grid::with_params(
            usdt_wallet_size,
            dec!(0.05),    // 5% bands
            14,            // TMA period
            dec!(0.005),   // 0.5% risk
            dec!(0.01),    // 1% grid spacing
            15             // 15 grid levels
        ),
        DefaultRiskManager::default(),
        market_stream,
        DefaultGlobalData::default(),
        |_| AlgorithmData::new(14),
    );

    // Build & run System:
    // See SystemBuilder for all configuration options
    let mut system = SystemBuilder::new(args)
        // Engine feed in Sync mode (Iterator input)
        .engine_feed_mode(EngineFeedMode::Iterator)
        // Audit feed is enabled (Engine sends audits)
        .audit_mode(AuditMode::Enabled)
        // Engine starts with TradingState::Disabled
        .trading_state(TradingState::Enabled)
        // Build System, but don't start spawning tasks yet
        .build::<EngineEvent, _>()?
        // Init System, spawning component tasks on the current runtime
        .init_with_runtime(tokio::runtime::Handle::current())
        .await?;

    // Take ownership of the Engine audit snapshot with updates
    let audit = system.audit.take().unwrap();

    // Run dummy asynchronous AuditStream consumer
    // Note: you probably want to use this Stream to replicate EngineState, or persist events, etc.
    //  --> eg/ see examples/engine_sync_with_audit_replica_engine_state
    let audit_task = tokio::spawn(async move {
        let mut audit_stream = audit.updates.into_stream();
        while let Some(audit) = audit_stream.next().await {
            debug!(?audit, "AuditStream consumed AuditTick");
            if audit.event.is_terminal() {
                break;
            }
        }
        audit_stream
    });

    // Enable trading
    system.trading_state(TradingState::Enabled);

    // Let the example run for 10 minutes...
    tokio::time::sleep(Duration::from_secs(600)).await;

    // Before shutting down, CancelOrders and then ClosePositions
    system.cancel_orders(InstrumentFilter::None);
    system.close_positions(InstrumentFilter::None);

    // Shutdown
    let (engine, _shutdown_audit) = system.shutdown().await?;
    let _audit_stream = audit_task.await?;

    // Generate TradingSummary<Daily>
    let trading_summary = engine
        .trading_summary_generator(RISK_FREE_RETURN)
        .generate(Daily);

    // Print TradingSummary<Daily> to terminal (could save in a file, send somewhere, etc.)
    trading_summary.print_summary();

    Ok(())
}

fn load_config() -> Result<SystemConfig, Box<dyn std::error::Error>> {
    let file = File::open(FILE_PATH_SYSTEM_CONFIG)?;
    let reader = BufReader::new(file);
    let config = serde_json::from_reader(reader)?;
    Ok(config)
}

/// Extracts the USDT wallet size from the system configuration
pub fn extract_usdt_wallet_size(config: &SystemConfig) -> Decimal {
    // Look through all executions to find USDT balance
    for execution in &config.executions {
        match execution {
            barter::system::config::ExecutionConfig::Mock(mock_config) => {
                // Search through balances to find USDT
                for balance in &mock_config.initial_state.balances {
                    // Convert AssetNameExchange to string and check if it's USDT
                    let asset_name = balance.asset.name().to_lowercase();
                    if asset_name == "usdt" {
                        return balance.balance.total;
                    }
                }
            }
            // Handle other execution config types if they exist
            _ => continue,
        }
    }

    // Default fallback if USDT not found
    println!("⚠️  USDT balance not found in config, using default: $10,000");
    dec!(10000)
}

