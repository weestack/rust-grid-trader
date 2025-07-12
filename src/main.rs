mod algorithms;
mod data;

use barter::{
    EngineEvent,
    engine::{
        Engine, Processor,
        clock::LiveClock,
        state::{
            EngineState,
            global::DefaultGlobalData,
            instrument::{
                data::{DefaultInstrumentMarketData, InstrumentDataState},
                filter::InstrumentFilter,
            },
            order::in_flight_recorder::InFlightRequestRecorder,
            position::PositionManager,
            trading::TradingState,
        },
    },
    logging::init_logging,
    risk::DefaultRiskManager,
    statistic::{summary::instrument::TearSheetGenerator, time::Daily},
    strategy::{
        DefaultStrategy,
        algo::AlgoStrategy,
        close_positions::{ClosePositionsStrategy, build_ioc_market_order_to_close_position},
        on_disconnect::OnDisconnectStrategy,
        on_trading_disabled::OnTradingDisabled,
    },
    system::{
        builder::{AuditMode, EngineFeedMode, SystemArgs, SystemBuilder},
        config::SystemConfig,
    },
};
use barter_data::{
    event::{DataKind, MarketEvent},
    streams::builder::dynamic::indexed::init_indexed_multi_exchange_market_stream,
    subscription::SubKind,
};
use barter_execution::{
    AccountEvent, AccountEventKind,
    order::{
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestCancel, OrderRequestOpen},
    },
};
use barter_instrument::{asset::AssetIndex, exchange::{ExchangeId, ExchangeIndex}, index::IndexedInstruments, instrument, instrument::InstrumentIndex};
use barter_integration::Terminal;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::{fs::File, io::BufReader, time::Duration};
use barter::engine::state::order::manager::OrderManager;
use tracing::debug;
use crate::data::{MultiStrategy, MultiStrategyCustomInstrumentData, StrategyA};

const FILE_PATH_SYSTEM_CONFIG: &str = "config/system_config.json";
const RISK_FREE_RETURN: Decimal = dec!(0.05);




#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialise Tracing
    init_logging();

    // Load SystemConfig
    let SystemConfig {
        instruments,
        executions,
    } = load_config()?;

    // Construct IndexedInstruments
    let instruments = IndexedInstruments::new(instruments);

    // Initialise MarketData Stream
    let market_stream = init_indexed_multi_exchange_market_stream(
        &instruments,
        &[SubKind::PublicTrades, SubKind::OrderBooksL1],
    )
        .await?;

    // Construct System Args
    let args = SystemArgs::new(
        &instruments,
        executions,
        LiveClock,
        StrategyA,
        DefaultRiskManager::default(),
        market_stream,
        DefaultGlobalData::default(),
        |_| MultiStrategyCustomInstrumentData::init(Utc::now()),
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

    // Let the example run for 5 seconds...
    tokio::time::sleep(Duration::from_secs(6000)).await;

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