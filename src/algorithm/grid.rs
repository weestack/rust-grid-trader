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
use barter_instrument::Side;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::collections::{HashMap, BTreeSet};
use std::sync::Mutex;
use chrono::Local;
use crate::algorithm::data::AlgorithmData;
use crate::algorithm::position::PositionSizer;

#[derive(Debug, Clone, PartialEq)]
enum GridZone {
    AboveHighBand,     // Price > High Band (overbought)
    BetweenBands,      // Low Band < Price < High Band (normal)
    BelowLowBand,      // Price < Low Band (oversold)
}

#[derive(Debug, Clone, PartialEq)]
enum TmaState {
    BullishTrend,      // Price consistently above TMA
    BearishTrend,      // Price consistently below TMA
    Sideways,          // Price oscillating around TMA
}

#[derive(Debug, Clone)]
enum SignalType {
    Buy,
    Sell,
    None,
}

#[derive(Debug, Clone)]
struct GridLevel {
    price: Decimal,
    level_type: GridLevelType,
    is_filled: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum GridLevelType {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
struct GridSignal {
    signal_type: SignalType,
    instrument_key: String,
    instrument_index: InstrumentIndex,
    exchange_index: ExchangeIndex,
    price: Decimal,
    tma: Decimal,
    high_band: Decimal,
    low_band: Decimal,
    volatility: Decimal,
    signal_source: String,
    grid_level: Option<Decimal>,
}

#[derive(Debug, Clone)]
struct InstrumentGridState {
    current_price: Decimal,
    price_history: Vec<Decimal>,
    buy_levels: BTreeSet<Decimal>,
    sell_levels: BTreeSet<Decimal>,
    filled_levels: BTreeSet<Decimal>,
    grid_spacing: Decimal,
    last_grid_zone: GridZone,
    last_tma_state: TmaState,
}

pub struct Grid {
    instrument_grids: Mutex<HashMap<String, InstrumentGridState>>,
    position_sizer: PositionSizer,
    band_percentage: Decimal,
    tma_period: usize,
    grid_spacing_percentage: Decimal,
    max_grid_levels: usize,
    price_history_length: usize,
}

impl Grid {
    #[allow(dead_code)]
    pub const ID: StrategyId = StrategyId(SmolStr::new_static("grid"));

    /// Creates a new Grid strategy with default parameters
    pub fn new(wallet_size: Decimal) -> Self {
        Self {
            instrument_grids: Mutex::new(HashMap::new()),
            position_sizer: PositionSizer::new(wallet_size),
            band_percentage: dec!(0.05), // 5% bands
            tma_period: 14,
            grid_spacing_percentage: dec!(0.02), // 2% spacing between grid levels
            max_grid_levels: 10,
            price_history_length: 50,
        }
    }

    /// Creates a new Grid strategy with custom parameters
    pub fn with_params(
        wallet_size: Decimal,
        band_percentage: Decimal,
        tma_period: usize,
        risk_percentage: Decimal,
        grid_spacing_percentage: Decimal,
        max_grid_levels: usize,
    ) -> Self {
        Self {
            instrument_grids: Mutex::new(HashMap::new()),
            position_sizer: PositionSizer::with_risk(wallet_size, risk_percentage),
            band_percentage,
            tma_period,
            grid_spacing_percentage,
            max_grid_levels,
            price_history_length: 50,
        }
    }

    /// Calculate Triangular Moving Average (TMA) from simple moving average
    fn calculate_tma(&self, sma: Decimal) -> Decimal {
        // For simplicity, we'll use the SMA as TMA base
        // In a real implementation, you'd calculate the double-smoothed average
        sma
    }

    /// Calculate High and Low Bands based on TMA
    fn calculate_bands(&self, tma: Decimal) -> (Decimal, Decimal) {
        let band_offset = tma * self.band_percentage;
        let high_band = tma + band_offset;
        let low_band = tma - band_offset;
        (high_band, low_band)
    }

    /// Calculate market volatility based on band width
    fn calculate_volatility(&self, high_band: Decimal, low_band: Decimal, tma: Decimal) -> Decimal {
        if tma == dec!(0) {
            return dec!(0);
        }
        ((high_band - low_band) / tma) * dec!(100)
    }

    /// Determine current grid zone based on price and bands
    fn determine_grid_zone(&self, price: Decimal, high_band: Decimal, low_band: Decimal) -> GridZone {
        if price > high_band {
            GridZone::AboveHighBand
        } else if price < low_band {
            GridZone::BelowLowBand
        } else {
            GridZone::BetweenBands
        }
    }

    /// Determine TMA trend state
    fn determine_tma_state(&self, price: Decimal, tma: Decimal, previous_price: Decimal) -> TmaState {
        let price_above_tma = price > tma;
        let previous_above_tma = previous_price > tma;

        match (price_above_tma, previous_above_tma) {
            (true, true) => TmaState::BullishTrend,
            (false, false) => TmaState::BearishTrend,
            _ => TmaState::Sideways,
        }
    }

    /// Calculate dynamic grid spacing based on price and volatility
    fn calculate_grid_spacing(&self, price: Decimal, volatility: Decimal) -> Decimal {
        let base_spacing = price * self.grid_spacing_percentage;

        // Adjust spacing based on volatility - higher volatility = wider spacing
        let volatility_multiplier = if volatility > dec!(5.0) {
            dec!(1.5) // 50% wider spacing for high volatility
        } else if volatility < dec!(2.0) {
            dec!(0.7) // 30% tighter spacing for low volatility
        } else {
            dec!(1.0) // Normal spacing
        };

        base_spacing * volatility_multiplier
    }

    /// Generate grid levels around current price
    fn generate_grid_levels(&self, current_price: Decimal, tma: Decimal, volatility: Decimal) -> (BTreeSet<Decimal>, BTreeSet<Decimal>) {
        let grid_spacing = self.calculate_grid_spacing(current_price, volatility);
        let mut buy_levels = BTreeSet::new();
        let mut sell_levels = BTreeSet::new();

        // Generate buy levels below current price
        for i in 1..=self.max_grid_levels {
            let buy_level = current_price - (grid_spacing * Decimal::from(i));
            if buy_level > dec!(0) {
                buy_levels.insert(buy_level);
            }
        }

        // Generate sell levels above current price
        for i in 1..=self.max_grid_levels {
            let sell_level = current_price + (grid_spacing * Decimal::from(i));
            sell_levels.insert(sell_level);
        }

        (buy_levels, sell_levels)
    }

    /// Update price history and maintain size limit
    fn update_price_history(&self, price_history: &mut Vec<Decimal>, new_price: Decimal) {
        price_history.push(new_price);
        if price_history.len() > self.price_history_length {
            price_history.remove(0);
        }
    }

    /// Find price ranges from historical data
    fn analyze_price_ranges(&self, price_history: &[Decimal]) -> (Decimal, Decimal, Decimal) {
        if price_history.is_empty() {
            return (dec!(0), dec!(0), dec!(0));
        }

        let min_price = price_history.iter().min().copied().unwrap_or(dec!(0));
        let max_price = price_history.iter().max().copied().unwrap_or(dec!(0));
        let avg_price = price_history.iter().sum::<Decimal>() / Decimal::from(price_history.len());

        (min_price, max_price, avg_price)
    }

    /// Check if current price crosses any grid levels
    fn check_grid_level_crosses(&self, grid_state: &InstrumentGridState, current_price: Decimal) -> Vec<(Decimal, GridLevelType)> {
        let mut crosses = Vec::new();
        let previous_price = grid_state.current_price;

        // Check buy level crosses (price moving down through buy levels)
        for &buy_level in &grid_state.buy_levels {
            if !grid_state.filled_levels.contains(&buy_level) {
                if previous_price > buy_level && current_price <= buy_level {
                    crosses.push((buy_level, GridLevelType::Buy));
                }
            }
        }

        // Check sell level crosses (price moving up through sell levels)
        for &sell_level in &grid_state.sell_levels {
            if !grid_state.filled_levels.contains(&sell_level) {
                if previous_price < sell_level && current_price >= sell_level {
                    crosses.push((sell_level, GridLevelType::Sell));
                }
            }
        }

        crosses
    }

    /// Check if we should generate a traditional grid signal (for fallback)
    fn should_generate_grid_signal(
        &self,
        previous_zone: &GridZone,
        current_zone: &GridZone,
        tma_state: &TmaState,
    ) -> bool {
        match (previous_zone, current_zone, tma_state) {
            // Buy signals: Price moves from below low band to between bands (oversold bounce)
            (GridZone::BelowLowBand, GridZone::BetweenBands, _) => true,
            // Sell signals: Price moves from above high band to between bands (overbought pullback)
            (GridZone::AboveHighBand, GridZone::BetweenBands, _) => true,
            // Additional signals based on trend
            (GridZone::BetweenBands, GridZone::BelowLowBand, TmaState::BullishTrend) => true, // Buy dip in uptrend
            (GridZone::BetweenBands, GridZone::AboveHighBand, TmaState::BearishTrend) => true, // Sell rally in downtrend
            _ => false,
        }
    }

    /// Get signal type based on zone transitions
    fn get_grid_signal_type(
        &self,
        previous_zone: &GridZone,
        current_zone: &GridZone,
        tma_state: &TmaState,
    ) -> SignalType {
        match (previous_zone, current_zone, tma_state) {
            // Buy signals
            (GridZone::BelowLowBand, GridZone::BetweenBands, _) => SignalType::Buy,
            (GridZone::BetweenBands, GridZone::BelowLowBand, TmaState::BullishTrend) => SignalType::Buy,
            // Sell signals
            (GridZone::AboveHighBand, GridZone::BetweenBands, _) => SignalType::Sell,
            (GridZone::BetweenBands, GridZone::AboveHighBand, TmaState::BearishTrend) => SignalType::Sell,
            _ => SignalType::None,
        }
    }

    fn process_instrument_signal(
        &self,
        instrument_state: &barter::engine::state::instrument::InstrumentState<AlgorithmData>,
    ) -> Vec<GridSignal> {
        // Get current price and moving average
        let Some(price) = instrument_state.data.price() else { return Vec::new(); };
        let Some(sma) = instrument_state.data.sma.value() else { return Vec::new(); };

        let instrument_key = instrument_state.instrument.name_exchange.name().to_string();
        let mut signals = Vec::new();

        // Calculate TMA from SMA
        let tma = self.calculate_tma(sma);

        // Calculate High and Low Bands
        let (high_band, low_band) = self.calculate_bands(tma);

        // Calculate volatility
        let volatility = self.calculate_volatility(high_band, low_band, tma);

        // Determine current grid zone
        let current_zone = self.determine_grid_zone(price, high_band, low_band);

        // Lock and get/update instrument grid state
        let mut instrument_grids = self.instrument_grids.lock().unwrap();
        let grid_state = instrument_grids.entry(instrument_key.clone()).or_insert_with(|| {
            InstrumentGridState {
                current_price: price,
                price_history: vec![price],
                buy_levels: BTreeSet::new(),
                sell_levels: BTreeSet::new(),
                filled_levels: BTreeSet::new(),
                grid_spacing: self.calculate_grid_spacing(price, volatility),
                last_grid_zone: GridZone::BetweenBands,
                last_tma_state: TmaState::Sideways,
            }
        });

        let previous_zone = grid_state.last_grid_zone.clone();
        let previous_price = grid_state.current_price;

        // Update price history
        self.update_price_history(&mut grid_state.price_history, price);

        // Analyze price ranges
        let (min_price, max_price, avg_price) = self.analyze_price_ranges(&grid_state.price_history);

        // Determine TMA state
        let tma_state = self.determine_tma_state(price, tma, previous_price);

        // Generate or update grid levels if this is a new instrument or price has moved significantly
        if grid_state.buy_levels.is_empty() || grid_state.sell_levels.is_empty() {
            let (buy_levels, sell_levels) = self.generate_grid_levels(price, tma, volatility);
            grid_state.buy_levels = buy_levels;
            grid_state.sell_levels = sell_levels;

            println!("[{}] 📊 GRID SETUP: {} | Price: {:.6} | TMA: {:.6} | Spacing: {:.6} | Buy Levels: {} | Sell Levels: {} | Range: {:.6}-{:.6}",
                     Local::now().format("%d-%m-%y %H:%M:%S"),
                     instrument_key,
                     price,
                     tma,
                     grid_state.grid_spacing,
                     grid_state.buy_levels.len(),
                     grid_state.sell_levels.len(),
                     min_price,
                     max_price
            );
        }

        // Check for grid level crosses
        let level_crosses = self.check_grid_level_crosses(&grid_state, price);

        // Generate signals for grid level crosses
        for (level_price, level_type) in level_crosses {
            let signal_type = match level_type {
                GridLevelType::Buy => SignalType::Buy,
                GridLevelType::Sell => SignalType::Sell,
            };

            let signal_source = format!("GRID_LEVEL_{:?}@{:.6}", level_type, level_price);

            signals.push(GridSignal {
                signal_type,
                instrument_key: instrument_key.clone(),
                instrument_index: instrument_state.key,
                exchange_index: instrument_state.instrument.exchange,
                price: level_price, // Use the grid level price, not current market price
                tma,
                high_band,
                low_band,
                volatility,
                signal_source,
                grid_level: Some(level_price),
            });

            // Mark this level as filled
            grid_state.filled_levels.insert(level_price);
        }

        // Fallback to traditional grid signals if no level crosses
        if signals.is_empty() {
            if self.should_generate_grid_signal(&previous_zone, &current_zone, &tma_state) {
                let signal_type = self.get_grid_signal_type(&previous_zone, &current_zone, &tma_state);

                if !matches!(signal_type, SignalType::None) {
                    let signal_source = format!("TRADITIONAL_{:?}->{:?}", previous_zone, current_zone);

                    signals.push(GridSignal {
                        signal_type,
                        instrument_key: instrument_key.clone(),
                        instrument_index: instrument_state.key,
                        exchange_index: instrument_state.instrument.exchange,
                        price,
                        tma,
                        high_band,
                        low_band,
                        volatility,
                        signal_source,
                        grid_level: None,
                    });
                }
            }
        }

        // Update grid state
        grid_state.current_price = price;
        grid_state.last_grid_zone = current_zone;
        grid_state.last_tma_state = tma_state;

        signals
    }

    fn create_buy_order(&self, signal: &GridSignal) -> OrderRequestOpen<ExchangeIndex, InstrumentIndex> {
        let quantity = self.position_sizer.calculate_quantity(signal.price);
        let position_value = self.position_sizer.calculate_position_value(signal.price);

        let level_info = if let Some(level) = signal.grid_level {
            format!("GridLevel@{:.6}", level)
        } else {
            "Market".to_string()
        };

        println!("[{}] 🟢 GRID BUY: {} @ {:.6} | Qty: {:.8} | Value: ${:.2} | TMA: {:.6} | Bands: [{:.6} - {:.6}] | Vol: {:.2}% | Source: {} | Level: {} | Risk: {:.2}%",
                 Local::now().format("%d-%m-%y %H:%M:%S"),
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 position_value,
                 signal.tma,
                 signal.low_band,
                 signal.high_band,
                 signal.volatility,
                 signal.signal_source,
                 level_info,
                 self.position_sizer.risk_percentage() * dec!(100)
        );

        OrderRequestOpen {
            key: OrderKey {
                exchange: signal.exchange_index,
                instrument: signal.instrument_index,
                strategy: Grid::ID,
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

    fn create_sell_order(&self, signal: &GridSignal) -> OrderRequestOpen<ExchangeIndex, InstrumentIndex> {
        let quantity = self.position_sizer.calculate_quantity(signal.price);
        let position_value = self.position_sizer.calculate_position_value(signal.price);

        let level_info = if let Some(level) = signal.grid_level {
            format!("GridLevel@{:.6}", level)
        } else {
            "Market".to_string()
        };

        println!("[{}] 🔴 GRID SELL: {} @ {:.6} | Qty: {:.8} | Value: ${:.2} | TMA: {:.6} | Bands: [{:.6} - {:.6}] | Vol: {:.2}% | Source: {} | Level: {} | Risk: {:.2}%",
                 Local::now().format("%d-%m-%y %H:%M:%S"),
                 signal.instrument_key,
                 signal.price,
                 quantity,
                 position_value,
                 signal.tma,
                 signal.low_band,
                 signal.high_band,
                 signal.volatility,
                 signal.signal_source,
                 level_info,
                 self.position_sizer.risk_percentage() * dec!(100)
        );

        OrderRequestOpen {
            key: OrderKey {
                exchange: signal.exchange_index,
                instrument: signal.instrument_index,
                strategy: Grid::ID,
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

impl Default for Grid {
    fn default() -> Self {
        Self::new(dec!(1000))
    }
}

impl AlgoStrategy for Grid {
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

        // Process all instruments and generate grid signals
        for instrument_state in state.instruments.instruments(&InstrumentFilter::None) {
            let signals = self.process_instrument_signal(instrument_state);

            for signal in signals {
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

impl ClosePositionsStrategy for Grid {
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

impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk> for Grid {
    type OnDisconnect = ();

    fn on_disconnect(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
        _: ExchangeId,
    ) -> Self::OnDisconnect {
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk> for Grid {
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
    }
}