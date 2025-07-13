use std::collections::VecDeque;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

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
            sample_interval: Duration::from_secs(5), // 1 minute intervals
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
        if self.value().is_some() {
            println!("price: {} RSI: {:?}", price, self.value());
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

    pub fn set_sample_interval(&mut self, interval: Duration) {
        self.sample_interval = interval;
    }
}