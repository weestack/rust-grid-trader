use barter::engine::Processor;
use barter::engine::state::instrument::data::{DefaultInstrumentMarketData, InstrumentDataState};
use barter::engine::state::order::in_flight_recorder::InFlightRequestRecorder;
use barter_data::event::{DataKind, MarketEvent};
use barter_execution::{AccountEvent, AccountEventKind};
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen};
use barter_instrument::exchange::ExchangeIndex;
use barter_instrument::instrument::InstrumentIndex;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::time::Duration;

use crate::algorithm::indicators::VwapIndicator;

#[derive(Debug, Clone)]
pub struct AlgorithmData {
    pub market_data: DefaultInstrumentMarketData,
    pub rsi: RSI,
    pub vwap: VwapIndicator,
}

impl AlgorithmData {
    pub fn new(rsi_period: usize) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            rsi: RSI::new(rsi_period),
            vwap: VwapIndicator::daily(), // Daily VWAP by default
        }
    }

    #[allow(dead_code)]
    pub fn new_with_vwap(rsi_period: usize, vwap_reset_period: Duration) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            rsi: RSI::new(rsi_period),
            vwap: VwapIndicator::new(vwap_reset_period),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RSI {
    period: usize,
    gains: VecDeque<Decimal>,
    losses: VecDeque<Decimal>,
    last_price: Option<Decimal>,
    avg_gain: Option<Decimal>,
    avg_loss: Option<Decimal>,
    last_update: Option<DateTime<Utc>>,
    sample_interval: Duration,
}

impl RSI {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            gains: VecDeque::new(),
            losses: VecDeque::new(),
            last_price: None,
            avg_gain: None,
            avg_loss: None,
            last_update: None,
            sample_interval: Duration::from_secs(10), // 5 minutes intervals for more meaningful price movements
        }
    }

    pub fn update_with_time(&mut self, price: Decimal, timestamp: DateTime<Utc>) {
        // Only update if enough time has passed
        if let Some(last_update) = self.last_update {
            if timestamp.signed_duration_since(last_update).to_std().unwrap_or_default() < self.sample_interval {
                return;
            }
        }

        if let Some(last_price) = self.last_price {
            let change = price - last_price;

            if change > dec!(0) {
                self.gains.push_back(change);
                self.losses.push_back(dec!(0));
            } else {
                self.gains.push_back(dec!(0));
                self.losses.push_back(change.abs());
            }

            // Keep only the required period
            if self.gains.len() > self.period {
                self.gains.pop_front();
                self.losses.pop_front();
            }

            // Calculate RSI when we have enough data
            if self.gains.len() == self.period {
                let avg_gain = self.gains.iter().sum::<Decimal>() / Decimal::from(self.period);
                let avg_loss = self.losses.iter().sum::<Decimal>() / Decimal::from(self.period);

                self.avg_gain = Some(avg_gain);
                self.avg_loss = Some(avg_loss);
            }
        }

        self.last_price = Some(price);
        self.last_update = Some(timestamp);
    }

    pub fn value(&self) -> Option<Decimal> {
        if let (Some(avg_gain), Some(avg_loss)) = (self.avg_gain, self.avg_loss) {
            if avg_loss == dec!(0) {
                return Some(dec!(100));
            }

            let rs = avg_gain / avg_loss;
            let rsi = dec!(100) - (dec!(100) / (dec!(1) + rs));
            Some(rsi)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn set_sample_interval(&mut self, interval: Duration) {
        self.sample_interval = interval;
    }
    
    #[allow(dead_code)]
    // Helper method to set common intervals
    pub fn set_minutes_interval(&mut self, minutes: u64) {
        self.sample_interval = Duration::from_secs(minutes * 60);
    }
}

impl InstrumentDataState for AlgorithmData {
    type MarketEventKind = DataKind;

    fn price(&self) -> Option<Decimal> {
        self.market_data.price()
    }
}

impl<InstrumentKey> Processor<&MarketEvent<InstrumentKey, DataKind>> for AlgorithmData {
    type Audit = ();

    fn process(&mut self, event: &MarketEvent<InstrumentKey, DataKind>) -> Self::Audit {
        // Process the market data first
        self.market_data.process(event);

        // Update RSI with time-based sampling when we have new price data
        if let Some(price) = self.market_data.price() {
            self.rsi.update_with_time(price, event.time_received);
        }

        // Update VWAP on trade events
        if let DataKind::Trade(trade) = &event.kind {
            self.vwap.update(Decimal::try_from(trade.price).unwrap(), Decimal::try_from(trade.amount).unwrap(), event.time_received);
        }
    }
}

impl Processor<&AccountEvent> for AlgorithmData {
    type Audit = ();

    fn process(&mut self, event: &AccountEvent) -> Self::Audit {
        // For now, just pass through - could add position tracking here if needed
        let AccountEventKind::Trade(_trade) = &event.kind else {
            return;
        };

        // Could add trade processing logic here if needed
    }
}

impl InFlightRequestRecorder for AlgorithmData {
    fn record_in_flight_cancel(&mut self, _: &OrderRequestCancel<ExchangeIndex, InstrumentIndex>) {}

    fn record_in_flight_open(&mut self, _: &OrderRequestOpen<ExchangeIndex, InstrumentIndex>) {}
}

impl Default for AlgorithmData {
    fn default() -> Self {
        Self::new(14) // Default RSI period of 14
    }
}