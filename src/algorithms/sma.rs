use barter::strategy::algo::AlgoStrategy;
use barter_execution::order::id::StrategyId;
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen, RequestOpen};
use barter::engine::state::instrument::data::InstrumentDataState;
use barter_instrument::Side;
use barter_instrument::{
    exchange::{ExchangeIndex, ExchangeId},
    instrument::InstrumentIndex,
};
use barter::engine::state::{EngineState};
use barter::engine::state::global::DefaultGlobalData;
use smol_str::SmolStr;
use std::collections::VecDeque;
use barter_execution::order::{OrderKey, OrderKind, TimeInForce};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

const SMA_PERIOD: usize = 5;

#[derive(Debug, Clone)]
pub struct SmaInstrumentData {
    closes: VecDeque<Decimal>
}

impl SmaInstrumentData {
    pub fn new() -> Self {
        Self { closes: VecDeque::with_capacity(SMA_PERIOD) }
    }

    pub fn update(&mut self, close: Decimal) {
        if self.closes.len() == SMA_PERIOD {
            self.closes.pop_front();
        }
        self.closes.push_back(close);
    }

    pub fn sma(&self) -> Option<Decimal> {
        if self.closes.len() < SMA_PERIOD {
            None
        } else {
            let sum: Decimal = self.closes.iter().cloned().sum();
            Some(sum / Decimal::from(SMA_PERIOD as u32))
        }
    }

    pub fn last_close(&self) -> Option<Decimal> {
        self.closes.back().copied()
    }
}

impl Default for SmaInstrumentData {
    fn default() -> Self {
        SmaInstrumentData::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SmaStrategyInstrumentState {
    pub instrument: SmaInstrumentData,
}

impl InstrumentDataState for SmaStrategyInstrumentState {
    type MarketEventKind = ();

    fn price(&self) -> Option<Decimal> {
        self.instrument.last_close()
    }
}

pub struct SmaStrategy;
impl SmaStrategy {
    const ID: StrategyId = StrategyId(SmolStr::new_static("sma-above"));
}

impl AlgoStrategy<ExchangeIndex, InstrumentIndex> for SmaStrategy {
    type State = EngineState<DefaultGlobalData, SmaStrategyInstrumentState>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        let mut open_orders = Vec::new();
        
        // Iterate over all instruments in the state
        // for ((exchange_idx, instrument_idx), data) in state.instruments.iter() {
        //     let sma = data.instrument.sma();
        //     let last = data.instrument.last_close();
        // 
        //     // Trade logic: Buy if last close > 5-SMA
        //     if let (Some(last), Some(sma)) = (last, sma) {
        //         if last > sma {
        //             
        //             open_orders.push(OrderRequestOpen::new(
        //                 OrderKey::new(
        //                     exchange_idx,
        //                     instrument_idx,
        //                     Self::ID,
        //                     Default::default(),
        //                 ),
        //                 RequestOpen::new(
        //                     Side::Buy,
        //                     dec!(1.5),
        //                     dec!(10),
        //                     OrderKind::Limit,
        //                     TimeInForce::GoodUntilEndOfDay,
        //                 )
        //             ));
        //             /*open_orders.push(OrderRequestOpen {
        //                 key: OrderKey {
        //                     exchange: (),
        //                     instrument: (),
        //                     strategy: StrategyId(),
        //                     cid: Default::default(),
        //                 },
        //                 exchange: *exchange_idx,
        //                 instrument: *instrument_idx,
        //                 side: Side::Buy,
        //                 order_type: OrderType::Market,
        //                 qty: dec!(1),  // size 1 for example
        //                 strategy_id: Self::ID,
        //                 // ...other fields as needed
        //                 client_order_id: barter_execution::order::id::ClientOrderId::random(),
        //                 price: None,
        //                 // fill other required fields appropriately for live or paper trading
        //                 state: (),
        //             });*/
        //         }
        //     }
        // }

        (std::iter::empty(), open_orders)
    }
}