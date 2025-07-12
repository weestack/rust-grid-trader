use barter::strategy::algo::AlgoStrategy;
use barter::engine::state::EngineState;
use barter::engine::state::global::DefaultGlobalData;
use barter_execution::order::{
    id::StrategyId,
    request::{OrderRequestCancel, OrderRequestOpen},
};
use barter_instrument::{
    instrument::InstrumentIndex,
    exchange::ExchangeIndex,
    Side,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use crate::MultiStrategyCustomInstrumentData;

const SMA_PERIOD: usize = 5;

pub struct SmaStrategy;

impl SmaStrategy {
    // Unique identifier for this strategy
    pub const ID: StrategyId = StrategyId(SmolStr::new_static("sma_strategy"));
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmaStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgoStrategy for SmaStrategy {
    // Defines the state the strategy needs to generate orders
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        let mut open_orders = Vec::new();

        // Iterate over all instruments the engine is trading
        for (instrument_index, instrument_state) in state.instruments.iter() {

            // We are using the data associated with `strategy_a` for our SMA calculations
            let sma_data = &instrument_state.data.strategy_a;

            // Ensure we have enough data to calculate an SMA
            if sma_data.closes.len() < SMA_PERIOD {
                continue;
            }

            // Get the most recent closing price
            let last_price = match sma_data.closes.back() {
                Some(price) => *price,
                None => continue,
            };

            // Calculate the 5-period SMA
            let sma: Decimal = sma_data.closes.iter().sum::<Decimal>() / Decimal::from(SMA_PERIOD);

            // Check if a position is already open for this strategy
            let position = &sma_data.position;

            // TRADING LOGIC:
            // If the last price is above the SMA and we don't have an open position,
            // generate a BUY market order.
            if last_price > sma && position.current.is_none() {
                let order = OrderRequestOpen::new_market(
                    Self::ID,
                    instrument_state.instrument.exchange,
                    *instrument_index,
                    Side::Buy,
                    dec!(0.01), // Example order size
                );

                open_orders.push(order);

                println!(
                    "SMA Strategy: BUY signal for {:?} at price {}. SMA is {}.",
                    instrument_state.instrument.name_exchange, last_price, sma
                );
            }
        }

        // This strategy doesn't cancel orders, so return an empty iterator.
        (Vec::<OrderRequestCancel<_,_>>::new(), open_orders)
    }
}