use barter::engine::Engine;
use barter::engine::state::EngineState;
use barter::engine::state::global::DefaultGlobalData;
use barter::engine::state::instrument::data::InstrumentDataState;
use barter::engine::state::instrument::filter::InstrumentFilter;
use barter::strategy::algo::AlgoStrategy;
use barter::strategy::close_positions::ClosePositionsStrategy;
use barter::strategy::on_disconnect::OnDisconnectStrategy;
use barter::strategy::on_trading_disabled::OnTradingDisabled;
use barter_execution::order::id::StrategyId;
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen, RequestOpen};
use barter_execution::order::{OrderKey, OrderKind, TimeInForce};
use barter_instrument::asset::AssetIndex;
use barter_instrument::exchange::{ExchangeId, ExchangeIndex};
use barter_instrument::instrument::InstrumentIndex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Mutex;
use barter_instrument::Side;
use crate::algorithm::data::AlgorithmData;

#[derive(Debug, Clone, PartialEq)]
enum RsiState {
    Neutral,
    Overbought,  // RSI > 80
    Oversold,    // RSI < 20
}

#[derive(Debug, Clone)]
enum SignalType {
    Buy,
    Sell,
    None,
}

#[derive(Debug, Clone)]
struct TradingSignal {
    signal_type: SignalType,
    instrument_key: String,
    instrument_index: InstrumentIndex,
    exchange_index: ExchangeIndex,
    price: Decimal,
    vwap: Decimal,
    rsi: Decimal,
    previous_rsi: Decimal,
}

pub struct Vwap {
    last_rsi_state: Mutex<HashMap<String, RsiState>>,
    last_rsi_value: Mutex<HashMap<String, Decimal>>,
}

impl Vwap {
    #[allow(dead_code)]
    pub const ID: StrategyId = StrategyId(SmolStr::new_static("vwap"));

    fn determine_rsi_state(rsi: Decimal) -> RsiState {
        if rsi > dec!(80) {
            RsiState::Overbought
        } else if rsi < dec!(20) {
            RsiState::Oversold
        } else {
            RsiState::Neutral
        }
    }

    fn should_generate_signal(previous_state: &RsiState, current_state: &RsiState) -> bool {
        matches!(
            (previous_state, current_state),
            (RsiState::Neutral, RsiState::Overbought) | (RsiState::Neutral, RsiState::Oversold)
        )
    }

    fn process_instrument_signal(
        &self,
        instrument_state: &barter::engine::state::instrument::InstrumentState<AlgorithmData>,
    ) -> Option<TradingSignal> {
        // Check if we have required data
        if instrument_state.data.vwap.value().is_none() || instrument_state.data.rsi.value().is_none() {
            return None;
        }

        let price = instrument_state.data.price()?;
        let rsi = instrument_state.data.rsi.value().unwrap_or(dec!(50));
        let vwap = instrument_state.data.vwap.value().unwrap();
        let instrument_key = instrument_state.instrument.name_exchange.name().to_string();

        // Determine current RSI state
        let current_state = Self::determine_rsi_state(rsi);

        // Lock and get previous states
        let mut last_states = self.last_rsi_state.lock().unwrap();
        let mut last_values = self.last_rsi_value.lock().unwrap();

        let previous_state = last_states.get(&instrument_key).cloned().unwrap_or(RsiState::Neutral);
        let previous_rsi = last_values.get(&instrument_key).cloned().unwrap_or(dec!(50));

        // Update state tracking
        last_states.insert(instrument_key.clone(), current_state.clone());
        last_values.insert(instrument_key.clone(), rsi);

        // Check if we should generate a signal
        if Self::should_generate_signal(&previous_state, &current_state) {
            let signal_type = match current_state {
                RsiState::Overbought => SignalType::Sell,
                RsiState::Oversold => SignalType::Buy,
                _ => SignalType::None,
            };

            Some(TradingSignal {
                signal_type,
                instrument_key,
                instrument_index: instrument_state.key,
                exchange_index: instrument_state.instrument.exchange,
                price,
                vwap,
                rsi,
                previous_rsi,
            })
        } else {
            None
        }
    }

    fn create_buy_order(&self, signal: &TradingSignal) -> OrderRequestOpen<ExchangeIndex, InstrumentIndex> {
        // Calculate order size (10% of USDT balance for simplicity)
        let order_value = dec!(1000) * dec!(0.005); // $1000 worth
        let quantity = order_value / signal.price;

        println!("🟢 BUY ORDER: {} @ {:.6} | Quantity: {:.6} | VWAP: {:.3} | RSI: {:.2} -> {:.2} [OVERSOLD]",
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 signal.vwap,
                 signal.previous_rsi,
                 signal.rsi
        );
        OrderRequestOpen {
            key: OrderKey {
                exchange: signal.exchange_index,
                instrument: signal.instrument_index,
                strategy: Vwap::ID,
                cid: Default::default(),
            },
            state: RequestOpen {
                side: Side::Buy,
                price: signal.price,
                quantity,
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodUntilEndOfDay,
            },
        }
    }

    fn create_sell_order(&self, signal: &TradingSignal) -> OrderRequestOpen<ExchangeIndex, InstrumentIndex> {
        // For sell orders, we'll sell a fixed amount (adjust based on your strategy)
        let quantity = match signal.instrument_key.as_str() {
            "BTCUSDT" => dec!(0.01),  // 0.01 BTC
            "ETHUSDT" => dec!(0.3),   // 0.3 ETH
            "SOLUSDT" => dec!(5.0),   // 5 SOL
            _ => dec!(0.01),          // Default
        };

        println!("🔴 SELL ORDER: {} @ {:.6} | Quantity: {:.6} | VWAP: {:.3} | RSI: {:.2} -> {:.2} [OVERBOUGHT]",
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 signal.vwap,
                 signal.previous_rsi,
                 signal.rsi
        );

        OrderRequestOpen {
            key: OrderKey {
                exchange: signal.exchange_index,
                instrument: signal.instrument_index,
                strategy: Vwap::ID,
                cid: Default::default(),
            },
            state: RequestOpen {
                side: Side::Sell,
                price: signal.price,
                quantity,
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodUntilEndOfDay,
            },
        }
    }
}

impl Default for Vwap {
    fn default() -> Self {
        Self {
            last_rsi_state: Mutex::new(HashMap::new()),
            last_rsi_value: Mutex::new(HashMap::new()),
        }
    }
}

impl AlgoStrategy for Vwap {
    type State = EngineState<DefaultGlobalData, AlgorithmData>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        let mut buy_orders = Vec::new();
        let mut sell_orders = Vec::new();

        // Process all instruments and generate signals
        for instrument_state in state.instruments.instruments(&InstrumentFilter::None) {
            if let Some(signal) = self.process_instrument_signal(instrument_state) {
                match signal.signal_type {
                    SignalType::Buy => {
                        buy_orders.push(self.create_buy_order(&signal));
                    },
                    SignalType::Sell => {
                        sell_orders.push(self.create_sell_order(&signal));
                    },
                    SignalType::None => {
                        // No action needed
                    }
                }
            }
        }

        // Combine buy and sell orders
        let all_orders = buy_orders.into_iter().chain(sell_orders.into_iter());

        (std::iter::empty(), all_orders)
    }
}

impl ClosePositionsStrategy for Vwap {
    type State = EngineState<DefaultGlobalData, AlgorithmData>;

    fn close_positions_requests<'a>(
        &'a self,
        _: &'a Self::State,
        _: &'a InstrumentFilter,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>> + 'a,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>> + 'a,
    )
    where
        ExchangeIndex: 'a,
        AssetIndex: 'a,
        InstrumentIndex: 'a,
    {
        (std::iter::empty(), std::iter::empty())
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk> for Vwap {
    type OnDisconnect = ();

    fn on_disconnect(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
        _: ExchangeId,
    ) -> Self::OnDisconnect {
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk> for Vwap {
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
    }
}