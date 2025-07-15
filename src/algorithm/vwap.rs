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
use chrono::Local;
use crate::algorithm::data::AlgorithmData;
use crate::algorithm::position::PositionSizer;

#[derive(Debug, Clone, PartialEq)]
enum RsiState {
    Neutral,
    Overbought,  // RSI > 80
    Oversold,    // RSI < 20
}

#[derive(Debug, Clone, PartialEq)]
enum VwapState {
    AboveVwap,    // Price > VWAP
    BelowVwap,    // Price < VWAP
    AtVwap,       // Price ≈ VWAP (within 0.1% threshold)
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
    signal_source: String, // "RSI" or "VWAP" or "COMBINED"
}

pub struct Vwap {
    last_rsi_state: Mutex<HashMap<String, RsiState>>,
    last_rsi_value: Mutex<HashMap<String, Decimal>>,
    last_vwap_state: Mutex<HashMap<String, VwapState>>,
    position_sizer: PositionSizer,
}

impl Vwap {
    #[allow(dead_code)]
    pub const ID: StrategyId = StrategyId(SmolStr::new_static("vwap"));

    // VWAP deviation threshold (0.1%)
    const VWAP_THRESHOLD: Decimal = dec!(0.001);

    /// Creates a new Vwap strategy with custom wallet size
    pub fn new(wallet_size: Decimal) -> Self {
        Self {
            last_rsi_state: Mutex::new(HashMap::new()),
            last_rsi_value: Mutex::new(HashMap::new()),
            last_vwap_state: Mutex::new(HashMap::new()),
            position_sizer: PositionSizer::new(wallet_size),
        }
    }

    /// Creates a new Vwap strategy with custom wallet size and risk percentage
    pub fn with_risk(wallet_size: Decimal, risk_percentage: Decimal) -> Self {
        Self {
            last_rsi_state: Mutex::new(HashMap::new()),
            last_rsi_value: Mutex::new(HashMap::new()),
            last_vwap_state: Mutex::new(HashMap::new()),
            position_sizer: PositionSizer::with_risk(wallet_size, risk_percentage),
        }
    }

    fn determine_rsi_state(rsi: Decimal) -> RsiState {
        if rsi > dec!(80) {
            RsiState::Overbought
        } else if rsi < dec!(20) {
            RsiState::Oversold
        } else {
            RsiState::Neutral
        }
    }

    fn determine_vwap_state(price: Decimal, vwap: Decimal) -> VwapState {
        let deviation = (price - vwap).abs() / vwap;

        if deviation <= Self::VWAP_THRESHOLD {
            VwapState::AtVwap
        } else if price > vwap {
            VwapState::AboveVwap
        } else {
            VwapState::BelowVwap
        }
    }

    fn should_generate_rsi_signal(previous_state: &RsiState, current_state: &RsiState) -> bool {
        matches!(
            (previous_state, current_state),
            (RsiState::Neutral, RsiState::Overbought) | (RsiState::Neutral, RsiState::Oversold)
        )
    }

    fn should_generate_vwap_signal(previous_state: &VwapState, current_state: &VwapState) -> bool {
        matches!(
            (previous_state, current_state),
            // Price crossing above VWAP (bullish)
            (VwapState::BelowVwap, VwapState::AboveVwap) |
            (VwapState::AtVwap, VwapState::AboveVwap) |
            // Price crossing below VWAP (bearish)
            (VwapState::AboveVwap, VwapState::BelowVwap) |
            (VwapState::AtVwap, VwapState::BelowVwap)
        )
    }

    fn get_vwap_signal_type(previous_state: &VwapState, current_state: &VwapState) -> SignalType {
        match (previous_state, current_state) {
            // Price crossing above VWAP = Buy signal
            (VwapState::BelowVwap, VwapState::AboveVwap) |
            (VwapState::AtVwap, VwapState::AboveVwap) => SignalType::Buy,
            // Price crossing below VWAP = Sell signal
            (VwapState::AboveVwap, VwapState::BelowVwap) |
            (VwapState::AtVwap, VwapState::BelowVwap) => SignalType::Sell,
            _ => SignalType::None,
        }
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

        // Determine current states
        let current_rsi_state = Self::determine_rsi_state(rsi);
        let current_vwap_state = Self::determine_vwap_state(price, vwap);

        // Lock and get previous states
        let mut last_rsi_states = self.last_rsi_state.lock().unwrap();
        let mut last_rsi_values = self.last_rsi_value.lock().unwrap();
        let mut last_vwap_states = self.last_vwap_state.lock().unwrap();

        let previous_rsi_state = last_rsi_states.get(&instrument_key).cloned().unwrap_or(RsiState::Neutral);
        let previous_rsi = last_rsi_values.get(&instrument_key).cloned().unwrap_or(dec!(50));
        let previous_vwap_state = last_vwap_states.get(&instrument_key).cloned().unwrap_or(VwapState::AtVwap);

        // Update state tracking
        last_rsi_states.insert(instrument_key.clone(), current_rsi_state.clone());
        last_rsi_values.insert(instrument_key.clone(), rsi);
        last_vwap_states.insert(instrument_key.clone(), current_vwap_state.clone());

        // Check for RSI signals
        let rsi_signal = if Self::should_generate_rsi_signal(&previous_rsi_state, &current_rsi_state) {
            match current_rsi_state {
                RsiState::Overbought => Some((SignalType::Sell, "RSI")),
                RsiState::Oversold => Some((SignalType::Buy, "RSI")),
                _ => None,
            }
        } else {
            None
        };

        // Check for VWAP signals
        let vwap_signal = if Self::should_generate_vwap_signal(&previous_vwap_state, &current_vwap_state) {
            let signal_type = Self::get_vwap_signal_type(&previous_vwap_state, &current_vwap_state);
            match signal_type {
                SignalType::None => None,
                _ => Some((signal_type, "VWAP")),
            }
        } else {
            None
        };

        // Prioritize signals: RSI signals take precedence over VWAP signals for stronger conviction
        let final_signal = match (rsi_signal, vwap_signal) {
            (Some((rsi_type, _)), Some((vwap_type, _))) => {
                // Both signals present - check if they agree
                if std::mem::discriminant(&rsi_type) == std::mem::discriminant(&vwap_type) {
                    Some((rsi_type, "COMBINED")) // Both agree, stronger signal
                } else {
                    Some((rsi_type, "RSI")) // RSI takes precedence when they disagree
                }
            },
            (Some((signal_type, source)), None) => Some((signal_type, source)),
            (None, Some((signal_type, source))) => Some((signal_type, source)),
            (None, None) => None,
        };

        // Return signal if we have one
        if let Some((signal_type, source)) = final_signal {
            Some(TradingSignal {
                signal_type,
                instrument_key,
                instrument_index: instrument_state.key,
                exchange_index: instrument_state.instrument.exchange,
                price,
                vwap,
                rsi,
                previous_rsi,
                signal_source: source.to_string(),
            })
        } else {
            None
        }
    }

    fn create_buy_order(&self, signal: &TradingSignal) -> OrderRequestOpen<ExchangeIndex, InstrumentIndex> {
        // Use position sizer to calculate exact quantity based on wallet size and 0.5% risk
        let quantity = self.position_sizer.calculate_quantity(signal.price);
        let position_value = self.position_sizer.calculate_position_value(signal.price);

        println!("[{}] 🟢 BUY ORDER: {} @ {:.6} | Quantity: {:.8} | Position Value: ${:.2} | Risk: {:.2}% | VWAP: {:.3} | RSI: {:.2} -> {:.2} | Source: {} [{}]",
                 Local::now().format("%d-%m-%y %H:%M:%S"),
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 position_value,
                 self.position_sizer.risk_percentage() * dec!(100),
                 signal.vwap,
                 signal.previous_rsi,
                 signal.rsi,
                 signal.signal_source,
                 if signal.price > signal.vwap { "ABOVE_VWAP" } else { "BELOW_VWAP" }
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
        // Use position sizer to calculate exact quantity based on wallet size and 0.5% risk
        let quantity = self.position_sizer.calculate_quantity(signal.price);
        let position_value = self.position_sizer.calculate_position_value(signal.price);

        println!("[{}] 🔴 SELL ORDER: {} @ {:.6} | Quantity: {:.8} | Position Value: ${:.2} | Risk: {:.2}% | VWAP: {:.3} | RSI: {:.2} -> {:.2} | Source: {} [{}]",
                 Local::now().format("%d-%m-%y %H:%M:%S"),
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 position_value,
                 self.position_sizer.risk_percentage() * dec!(100),
                 signal.vwap,
                 signal.previous_rsi,
                 signal.rsi,
                 signal.signal_source,
                 if signal.price > signal.vwap { "ABOVE_VWAP" } else { "BELOW_VWAP" }
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
        Self::new(dec!(1000))
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