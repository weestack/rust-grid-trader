use barter::engine::Processor;
use barter::engine::state::instrument::data::{DefaultInstrumentMarketData, InstrumentDataState};
use barter::engine::state::order::in_flight_recorder::InFlightRequestRecorder;
use barter_data::event::{DataKind, MarketEvent};
use barter_execution::{AccountEvent, AccountEventKind};
use barter_execution::order::request::{OrderRequestCancel, OrderRequestOpen};
use barter_instrument::exchange::ExchangeIndex;
use barter_instrument::instrument::InstrumentIndex;
use rust_decimal::Decimal;
use std::time::Duration;
use crate::algorithm::indicators::rsi::RSI;
use crate::algorithm::indicators::VwapIndicator;
use crate::algorithm::indicators::sma::SMA;

#[derive(Debug, Clone)]
pub struct AlgorithmData {
    pub market_data: DefaultInstrumentMarketData,
    pub rsi: RSI,
    pub vwap: VwapIndicator,
    pub sma: SMA,
}

impl AlgorithmData {
    pub fn new(rsi_period: usize) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            rsi: RSI::new(rsi_period),
            vwap: VwapIndicator::daily(), // Daily VWAP by default
            sma: SMA::new(14), // Default SMA period of 14
        }
    }

    pub fn new_with_periods(rsi_period: usize, sma_period: usize) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            rsi: RSI::new(rsi_period),
            vwap: VwapIndicator::daily(),
            sma: SMA::new(sma_period),
        }
    }

    #[allow(dead_code)]
    pub fn new_with_vwap(rsi_period: usize, vwap_reset_period: Duration) -> Self {
        Self {
            market_data: DefaultInstrumentMarketData::default(),
            rsi: RSI::new(rsi_period),
            vwap: VwapIndicator::new(vwap_reset_period),
            sma: SMA::new(14),
        }
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

        // Update indicators with new price data
        if let Some(price) = self.market_data.price() {
            self.rsi.update_with_time(price, event.time_received);
            self.sma.update(price);
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