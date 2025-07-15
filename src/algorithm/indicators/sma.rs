use rust_decimal::Decimal;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct SMA {
    period: usize,
    prices: VecDeque<Decimal>,
    current_value: Option<Decimal>,
}

impl SMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prices: VecDeque::with_capacity(period),
            current_value: None,
        }
    }

    pub fn update(&mut self, price: Decimal) {
        self.prices.push_back(price);

        // Remove oldest price if we exceed the period
        if self.prices.len() > self.period {
            self.prices.pop_front();
        }

        // Calculate SMA if we have enough data
        if self.prices.len() >= self.period {
            let sum: Decimal = self.prices.iter().sum();
            self.current_value = Some(sum / Decimal::from(self.period));
        }
    }

    pub fn value(&self) -> Option<Decimal> {
        self.current_value
    }

    pub fn is_ready(&self) -> bool {
        self.prices.len() >= self.period
    }

    pub fn reset(&mut self) {
        self.prices.clear();
        self.current_value = None;
    }
}

impl Default for SMA {
    fn default() -> Self {
        Self::new(14)
    }
}