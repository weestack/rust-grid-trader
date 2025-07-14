use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VwapIndicator {
    price_volume_sum: Decimal,
    volume_sum: Decimal,
    trades: VecDeque<VwapTrade>,
    reset_period: Duration,
    last_reset: Option<DateTime<Utc>>,
    current_vwap: Option<Decimal>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct VwapTrade {
    price: Decimal,
    volume: Decimal,
    timestamp: DateTime<Utc>,
}

impl VwapIndicator {
    pub fn new(reset_period: Duration) -> Self {
        Self {
            price_volume_sum: dec!(0),
            volume_sum: dec!(0),
            trades: VecDeque::new(),
            reset_period,
            last_reset: None,
            current_vwap: None,
        }
    }

    /// Create a daily VWAP (resets every 24 hours)
    pub fn daily() -> Self {
        Self::new(Duration::from_secs(24 * 60 * 60))
    }

    /// Create an hourly VWAP (resets every hour)
    pub fn _hourly() -> Self {
        Self::new(Duration::from_secs(60 * 60))
    }

    /// Create a session VWAP (resets every 8 hours)
    pub fn _session() -> Self {
        Self::new(Duration::from_secs(8 * 60 * 60))
    }

    pub fn update(&mut self, price: Decimal, volume: Decimal, timestamp: DateTime<Utc>) {
        // Check if we need to reset the VWAP
        if self.should_reset(timestamp) {
            self.reset(timestamp);
        }

        // Add the new trade
        let trade = VwapTrade {
            price,
            volume,
            timestamp,
        };

        self.trades.push_back(trade);
        self.price_volume_sum += price * volume;
        self.volume_sum += volume;

        // Calculate VWAP
        if self.volume_sum > dec!(0) {
            self.current_vwap = Some(self.price_volume_sum / self.volume_sum);
        }
    }

    pub fn value(&self) -> Option<Decimal> {
        self.current_vwap
    }

    #[allow(dead_code)]
    pub fn total_volume(&self) -> Decimal {
        self.volume_sum
    }

    #[allow(dead_code)]
    pub fn trade_count(&self) -> usize {
        self.trades.len()
    }

    fn should_reset(&self, timestamp: DateTime<Utc>) -> bool {
        if let Some(last_reset) = self.last_reset {
            timestamp.signed_duration_since(last_reset).to_std().unwrap_or_default() >= self.reset_period
        } else {
            true // First update
        }
    }

    fn reset(&mut self, timestamp: DateTime<Utc>) {
        self.price_volume_sum = dec!(0);
        self.volume_sum = dec!(0);
        self.trades.clear();
        self.current_vwap = None;
        self.last_reset = Some(timestamp);
    }

    #[allow(dead_code)]
    pub fn set_reset_period(&mut self, period: Duration) {
        self.reset_period = period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[test]
    fn test_vwap_calculation() {
        let mut vwap = VwapIndicator::new(Duration::from_secs(3600));
        let now = Utc::now();

        // Add some trades
        vwap.update(dec!(100), dec!(10), now);
        vwap.update(dec!(102), dec!(20), now);
        vwap.update(dec!(98), dec!(30), now);

        // VWAP = (100*10 + 102*20 + 98*30) / (10+20+30)
        // VWAP = (1000 + 2040 + 2940) / 60 = 5980 / 60 = 99.67
        let expected_vwap = dec!(5980) / dec!(60);
        assert_eq!(vwap.value(), Some(expected_vwap));
        assert_eq!(vwap.total_volume(), dec!(60));
        assert_eq!(vwap.trade_count(), 3);
    }

    #[test]
    fn test_vwap_reset() {
        let mut vwap = VwapIndicator::new(Duration::from_secs(1));
        let now = Utc::now();

        vwap.update(dec!(100), dec!(10), now);
        assert!(vwap.value().is_some());

        // Wait for reset period and add new trade
        let later = now + chrono::Duration::seconds(2);
        vwap.update(dec!(200), dec!(5), later);

        // Should only have the new trade
        assert_eq!(vwap.value(), Some(dec!(200)));
        assert_eq!(vwap.total_volume(), dec!(5));
        assert_eq!(vwap.trade_count(), 1);
    }
}