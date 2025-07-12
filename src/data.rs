use std::ops::Add;
use barter::engine::{Engine, Processor};
use barter::engine::state::EngineState;
use barter::engine::state::global::DefaultGlobalData;
use barter::engine::state::instrument::data::{DefaultInstrumentMarketData, InstrumentDataState};
use barter::engine::state::instrument::filter::InstrumentFilter;
use barter::engine::state::instrument::InstrumentState;
use barter::engine::state::order::in_flight_recorder::InFlightRequestRecorder;
use barter::engine::state::position::PositionManager;
use barter::statistic::summary::instrument::TearSheetGenerator;
use barter::strategy::algo::AlgoStrategy;
use barter::strategy::close_positions::{build_ioc_market_order_to_close_position, ClosePositionsStrategy};
use barter::strategy::on_disconnect::OnDisconnectStrategy;
use barter::strategy::on_trading_disabled::OnTradingDisabled;
use barter_data::event::{DataKind, MarketEvent};
use barter_execution::{AccountEvent, AccountEventKind};
use barter_execution::order::id::{ClientOrderId, StrategyId};
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen};
use barter_instrument::asset::AssetIndex;
use barter_instrument::exchange::{ExchangeId, ExchangeIndex};
use barter_instrument::instrument::InstrumentIndex;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use smol_str::SmolStr;

pub struct MultiStrategy {
    strategy_a: StrategyA,
    strategy_b: StrategyB,
}
#[derive(Debug, Clone)]
struct StrategyCustomInstrumentData {
    tear: TearSheetGenerator,
    position: PositionManager,
}

impl StrategyCustomInstrumentData {
    pub fn init(time_engine_start: DateTime<Utc>) -> Self {
        Self {
            tear: TearSheetGenerator::init(time_engine_start),
            position: PositionManager::default(),
        }
    }
}

impl AlgoStrategy for MultiStrategy {
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        let (cancels_a, opens_a) = self.strategy_a.generate_algo_orders(state);
        let (cancels_b, opens_b) = self.strategy_b.generate_algo_orders(state);

        let cancels_all = cancels_a.into_iter().chain(cancels_b);
        let opens_all = opens_a.into_iter().chain(opens_b);

        (cancels_all, opens_all)
    }
}

impl ClosePositionsStrategy for MultiStrategy {
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

    fn close_positions_requests<'a>(
        &'a self,
        state: &'a Self::State,
        filter: &'a InstrumentFilter,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel> + 'a,
        impl IntoIterator<Item = OrderRequestOpen> + 'a,
    )
    where
        ExchangeIndex: 'a,
        AssetIndex: 'a,
        InstrumentIndex: 'a,
    {
        // Generate a MARKET order for each Strategy's open Position
        let open_requests =
            state
                .instruments
                .instruments(filter)
                .flat_map(move |state| {
                    // Only generate orders if we have a market price
                    let Some(price) = state.data.price() else {
                        return itertools::Either::Left(std::iter::empty());
                    };

                    // Generate a MARKET order to close StrategyA position
                    let close_position_a_request = state
                        .data
                        .strategy_a
                        .position
                        .current
                        .as_ref()
                        .map(|position_a| {
                            build_ioc_market_order_to_close_position(
                                state.instrument.exchange,
                                position_a,
                                StrategyA::ID,
                                price,
                                || ClientOrderId::random(),
                            )
                        });

                    // Generate a MARKET order to close StrategyB position
                    let close_position_b_request = state
                        .data
                        .strategy_b
                        .position
                        .current
                        .as_ref()
                        .map(|position_b| {
                            build_ioc_market_order_to_close_position(
                                state.instrument.exchange,
                                position_b,
                                StrategyB::ID,
                                price,
                                || ClientOrderId::random(),
                            )
                        });

                    itertools::Either::Right(
                        close_position_a_request
                            .into_iter()
                            .chain(close_position_b_request),
                    )
                });

        (std::iter::empty(), open_requests)
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk>
for MultiStrategy
{
    type OnDisconnect = ();

    fn on_disconnect(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
        _: ExchangeId,
    ) -> Self::OnDisconnect {
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk>
for MultiStrategy
{
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
    }
}

#[derive(Debug)]
pub struct StrategyA;

impl StrategyA {
    const ID: StrategyId = StrategyId(SmolStr::new_static("strategy_a"));
}

impl ClosePositionsStrategy for StrategyA {
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

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

impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk> for StrategyA {
    type OnDisconnect = ();

    fn on_disconnect(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
        _: ExchangeId,
    ) -> Self::OnDisconnect {
    }
}

struct StrategyB;

impl StrategyB {
    const ID: StrategyId = StrategyId(SmolStr::new_static("strategy_b"));
}

impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk> for StrategyA {
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
    }
}

impl InstrumentDataState for MultiStrategyCustomInstrumentData {
    type MarketEventKind = DataKind;

    fn price(&self) -> Option<Decimal> {
        self.market_data.price()
    }
}

impl Processor<&AccountEvent> for MultiStrategyCustomInstrumentData {
    type Audit = ();

    fn process(&mut self, event: &AccountEvent) -> Self::Audit {
        let AccountEventKind::Trade(trade) = &event.kind else {
            return;
        };

        if trade.strategy == StrategyA::ID {
            self.strategy_a
                .position
                .update_from_trade(trade)
                .inspect(|closed| self.strategy_a.tear.update_from_position(closed));
        }

        if trade.strategy == StrategyB::ID {
            self.strategy_b
                .position
                .update_from_trade(trade)
                .inspect(|closed| self.strategy_b.tear.update_from_position(closed));
        }
    }
}

impl InFlightRequestRecorder for MultiStrategyCustomInstrumentData {
    fn record_in_flight_cancel(&mut self, _: &OrderRequestCancel<ExchangeIndex, InstrumentIndex>) {}

    fn record_in_flight_open(&mut self, _: &OrderRequestOpen<ExchangeIndex, InstrumentIndex>) {}
}

impl Default for StrategyCustomInstrumentData {
    fn default() -> Self {
        Self {
            tear: TearSheetGenerator::init(DateTime::<Utc>::MIN_UTC),
            position: Default::default(),
        }
    }
}


#[derive(Debug, Clone, Default)]
pub struct MultiStrategyCustomInstrumentData {
    market_data: DefaultInstrumentMarketData,
    strategy_a: StrategyCustomInstrumentData,
    strategy_b: StrategyCustomInstrumentData,

    // Shared calculations accessible by both strategies
    shared_indicators: SharedIndicators,
    shared_metrics: SharedMetrics,
}


#[derive(Debug, Clone, Default)]
struct SharedIndicators {
    moving_average_20: Option<Decimal>,
    moving_average_50: Option<Decimal>,
    rsi: Option<Decimal>,
    volatility: Option<Decimal>,
    price_history: Vec<Decimal>, // Last N prices for calculations
}

#[derive(Debug, Clone, Default)]
struct SharedMetrics {
    total_volume: Decimal,
    price_momentum: Option<Decimal>,
    market_trend: MarketTrend,
}

#[derive(Debug, Clone, Default)]
enum MarketTrend {
    #[default]
    Neutral,
    Bullish,
    Bearish,
}

impl MultiStrategyCustomInstrumentData {
    pub fn init(time_engine_start: DateTime<Utc>) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            strategy_a: StrategyCustomInstrumentData::init(time_engine_start),
            strategy_b: StrategyCustomInstrumentData::init(time_engine_start),
            shared_indicators: SharedIndicators::default(),
            shared_metrics: SharedMetrics::default(),
        }
    }

    // Helper method to update shared calculations
    fn update_shared_calculations(&mut self, price: Decimal) {
        // Update price history
        self.shared_indicators.price_history.push(price);
        if self.shared_indicators.price_history.len() > 50 {
            self.shared_indicators.price_history.remove(0);
        }

        // Calculate moving averages
        if self.shared_indicators.price_history.len() >= 20 {
            let sum_20: Decimal = self.shared_indicators.price_history.iter().rev().take(20).sum();
            self.shared_indicators.moving_average_20 = Some(sum_20 / dec!(20));
        }

        if self.shared_indicators.price_history.len() >= 50 {
            let sum_50: Decimal = self.shared_indicators.price_history.iter().sum();
            self.shared_indicators.moving_average_50 = Some(sum_50 / Decimal::from(self.shared_indicators.price_history.len()));
        }

        // Update market trend based on moving averages
        if let (Some(ma_20), Some(ma_50)) = (self.shared_indicators.moving_average_20, self.shared_indicators.moving_average_50) {
            self.shared_metrics.market_trend = if ma_20 > ma_50 {
                MarketTrend::Bullish
            } else if ma_20 < ma_50 {
                MarketTrend::Bearish
            } else {
                MarketTrend::Neutral
            };
        }
    }
}

impl<InstrumentKey> Processor<&MarketEvent<InstrumentKey, DataKind>>
for MultiStrategyCustomInstrumentData
{
    type Audit = ();

    fn process(&mut self, event: &MarketEvent<InstrumentKey, DataKind>) -> Self::Audit {
        // Process the market data as before
        self.market_data.process(event);

        // Update shared calculations when we have new price data
        if let Some(price) = self.market_data.price() {
            self.update_shared_calculations(price);
        }

        // Update volume metrics
        //if let DataKind::Trade(trade) = &event.kind {
        //    self.shared_metrics.total_volume += Decimal::f;
        //}
    }
}

impl AlgoStrategy for StrategyA {
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        for instrument_state in state.instruments.instruments(&InstrumentFilter::None) {
            let shared_data = &instrument_state.data;

            // Access shared indicators
            let ma_20 = shared_data.shared_indicators.moving_average_20;
            let ma_50 = shared_data.shared_indicators.moving_average_50;
            let market_trend = &shared_data.shared_metrics.market_trend;

            println!("StrategyA {:?} - MA20: {:?}, MA50: {:?}, Trend: {:?}", instrument_state.instrument.name_exchange.name(), ma_20, ma_50, market_trend);

            // Strategy A logic using shared data
            match market_trend {
                MarketTrend::Bullish => {
                    // Generate buy orders for Strategy A
                    println!("StrategyA: Bullish trend detected");
                },
                MarketTrend::Bearish => {
                    // Generate sell orders for Strategy A
                    println!("StrategyA: Bearish trend detected");
                },
                MarketTrend::Neutral => {
                    // No action or different logic
                    println!("StrategyA: Neutral trend");
                }
            }
        }
        //let instrument_state: &InstrumentState<MultiStrategyCustomInstrumentData> = state.instruments.instruments(&InstrumentFilter::None).next().unwrap();

        (std::iter::empty(), std::iter::empty())
    }
}

impl AlgoStrategy for StrategyB {
    type State = EngineState<DefaultGlobalData, MultiStrategyCustomInstrumentData>;

    fn generate_algo_orders(
        &self,
        state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        let instrument_state: &InstrumentState<MultiStrategyCustomInstrumentData> = state.instruments.instruments(&InstrumentFilter::None).next().unwrap();
        let shared_data = &instrument_state.data;

        // Strategy B can use the same shared calculations
        let volatility = shared_data.shared_indicators.volatility;
        let total_volume = shared_data.shared_metrics.total_volume;

        println!("StrategyB - Volatility: {:?}, Total Volume: {:?}", volatility, total_volume);

        // Strategy B logic that might complement Strategy A
        if let Some(vol) = volatility {
            if vol > dec!(0.05) {
                println!("StrategyB: High volatility detected - implementing risk management");
            }
        }

        (std::iter::empty(), std::iter::empty())
    }
}

#[derive(Debug, Clone, Default)]
struct StrategyCoordination {
    strategy_a_signal: Option<TradingSignal>,
    strategy_b_signal: Option<TradingSignal>,
    combined_signal: Option<TradingSignal>,
    risk_override: bool,
}

#[derive(Debug, Clone)]
enum TradingSignal {
    Buy,
    Sell,
    Hold,
}

impl MultiStrategyCustomInstrumentData {
    // Method to coordinate between strategies
    fn coordinate_strategies(&mut self) {
        println!("Coordinate strategies");
    }
}