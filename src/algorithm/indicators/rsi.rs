
use std::collections::VecDeque;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct RSI {
    period: usize,
    gains: VecDeque<(Decimal, DateTime<Utc>)>,
    losses: VecDeque<(Decimal, DateTime<Utc>)>,
    last_price: Option<Decimal>,
    avg_gain: Option<Decimal>,
    avg_loss: Option<Decimal>,
    last_update: Option<DateTime<Utc>>,
    window_duration: Duration,
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
            window_duration: Duration::from_secs(180), // 60 second rolling window
        }
    }

    fn remove_old_entries(queue: &mut VecDeque<(Decimal, DateTime<Utc>)>, cutoff_time: DateTime<Utc>) {
        while let Some((_, old_timestamp)) = queue.front() {
            if *old_timestamp < cutoff_time {
                queue.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn update_with_time(&mut self, price: Decimal, timestamp: DateTime<Utc>) {
        if let Some(last_price) = self.last_price {
            if last_price == price {
                // prevent duplicates
                return;
            }
            
            let change = price - last_price;

            if change > dec!(0) {
                self.gains.push_back((change, timestamp));
                self.losses.push_back((dec!(0), timestamp));
            } else {
                self.gains.push_back((dec!(0), timestamp));
                self.losses.push_back((change.abs(), timestamp));
            }

            // Remove entries older than 60 seconds
            let cutoff_time = timestamp - chrono::Duration::from_std(self.window_duration).unwrap_or_default();
            
            // keep for the duration of the rolling window, not the period
            Self::remove_old_entries(&mut self.gains, cutoff_time);
            Self::remove_old_entries(&mut self.losses, cutoff_time);

            // Calculate RSI when we have enough data
            if self.gains.len() >= self.period {
                let avg_gain = self.gains.iter().map(|(gain, _)| *gain).sum::<Decimal>() / Decimal::from(self.gains.len());
                let avg_loss = self.losses.iter().map(|(loss, _)| *loss).sum::<Decimal>() / Decimal::from(self.losses.len());

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
    pub fn set_window(&mut self, interval: Duration) {
        self.window_duration = interval;
    }
}