
use barter::engine::Engine;
use barter::engine::state::EngineState;
use barter::engine::state::global::DefaultGlobalData;
use barter::engine::state::instrument::data::{DefaultInstrumentMarketData, InstrumentDataState};
use barter::engine::state::instrument::filter::InstrumentFilter;
use barter::strategy::algo::AlgoStrategy;
use barter::strategy::close_positions::ClosePositionsStrategy;
use barter::strategy::on_disconnect::OnDisconnectStrategy;
use barter::strategy::on_trading_disabled::OnTradingDisabled;
use barter_execution::order::id::StrategyId;
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen};
use barter_instrument::asset::AssetIndex;
use barter_instrument::exchange::{ExchangeId, ExchangeIndex};
use barter_instrument::instrument::InstrumentIndex;
use smol_str::SmolStr;
use crate::algorithm::data::AlgorithmData;

pub struct Vwap;

impl Vwap {
    pub const ID: StrategyId = StrategyId(SmolStr::new_static("vwap"));
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
        // Basic VWAP logic - print current prices for now
        for instrument_state in state.instruments.instruments(&InstrumentFilter::None) {
            if instrument_state.data.vwap.value().is_none() || instrument_state.data.rsi.value().is_none() {
                continue;
            }
            if let Some(price) = instrument_state.data.price() {

                println!("{:<10}: {:>8.3}, RSI: {:>6.3}, VWAP: {:>8.3}",
                         instrument_state.instrument.name_exchange.name(),
                         price,
                         instrument_state.data.rsi.value().unwrap(),
                         instrument_state.data.vwap.value().unwrap()
                );
            }
        }

        (std::iter::empty(), std::iter::empty())
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