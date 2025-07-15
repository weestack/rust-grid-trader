use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct PositionSizer {
    wallet_size: Decimal,
    risk_percentage: Decimal,
}

impl PositionSizer {
    /// Creates a new PositionSizer with the specified wallet size and 0.5% risk
    pub fn new(wallet_size: Decimal) -> Self {
        Self {
            wallet_size,
            risk_percentage: dec!(0.005), // 0.5%
        }
    }

    /// Creates a new PositionSizer with custom risk percentage
    pub fn with_risk(wallet_size: Decimal, risk_percentage: Decimal) -> Self {
        Self {
            wallet_size,
            risk_percentage,
        }
    }

    /// Calculates the exact quantity to buy based on wallet size and risk percentage
    ///
    /// # Arguments
    /// * `price` - Current price of the asset
    ///
    /// # Returns
    /// * `Decimal` - The exact quantity to purchase
    pub fn calculate_quantity(&self, price: Decimal) -> Decimal {
        if price <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        // Calculate the dollar amount to risk (wallet_size * risk_percentage)
        let risk_amount = self.wallet_size * self.risk_percentage;

        // Calculate quantity: risk_amount / price
        let quantity = risk_amount / price;

        // Round to appropriate precision (8 decimal places for crypto)
        quantity.round_dp(8)
    }

    /// Calculates the dollar value of the position
    ///
    /// # Arguments
    /// * `price` - Current price of the asset
    ///
    /// # Returns
    /// * `Decimal` - The dollar value of the calculated position
    pub fn calculate_position_value(&self, price: Decimal) -> Decimal {
        let quantity = self.calculate_quantity(price);
        quantity * price
    }

    /// Updates the wallet size (useful for dynamic position sizing)
    pub fn update_wallet_size(&mut self, new_wallet_size: Decimal) {
        self.wallet_size = new_wallet_size;
    }

    /// Updates the risk percentage
    pub fn update_risk_percentage(&mut self, new_risk_percentage: Decimal) {
        self.risk_percentage = new_risk_percentage;
    }

    /// Gets the current wallet size
    pub fn wallet_size(&self) -> Decimal {
        self.wallet_size
    }

    /// Gets the current risk percentage
    pub fn risk_percentage(&self) -> Decimal {
        self.risk_percentage
    }

    /// Gets the current risk amount in dollars
    pub fn risk_amount(&self) -> Decimal {
        self.wallet_size * self.risk_percentage
    }
}

impl Default for PositionSizer {
    fn default() -> Self {
        Self::new(dec!(50000)) // Default to $50,000 wallet size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_sizer_calculation() {
        let sizer = PositionSizer::new(dec!(50000)); // $50,000 wallet

        // Test BTC at $50,000
        let btc_price = dec!(50000);
        let btc_quantity = sizer.calculate_quantity(btc_price);
        let expected_btc = dec!(250) / btc_price; // $250 / $50,000 = 0.005 BTC
        assert_eq!(btc_quantity, expected_btc);

        // Test ETH at $2,500
        let eth_price = dec!(2500);
        let eth_quantity = sizer.calculate_quantity(eth_price);
        let expected_eth = dec!(250) / eth_price; // $250 / $2,500 = 0.1 ETH
        assert_eq!(eth_quantity, expected_eth);

        // Test position value
        let position_value = sizer.calculate_position_value(btc_price);
        assert_eq!(position_value, dec!(250)); // Should always be $250 (0.5% of $50,000)
    }

    #[test]
    fn test_custom_risk_percentage() {
        let sizer = PositionSizer::with_risk(dec!(100000), dec!(0.01)); // $100,000 wallet, 1% risk

        let price = dec!(1000);
        let quantity = sizer.calculate_quantity(price);
        let expected = dec!(1000) / price; // $1,000 / $1,000 = 1.0
        assert_eq!(quantity, expected);
    }

    #[test]
    fn test_zero_price() {
        let sizer = PositionSizer::new(dec!(50000));
        let quantity = sizer.calculate_quantity(dec!(0));
        assert_eq!(quantity, dec!(0));
    }

    #[test]
    fn test_update_methods() {
        let mut sizer = PositionSizer::new(dec!(50000));

        sizer.update_wallet_size(dec!(100000));
        assert_eq!(sizer.wallet_size(), dec!(100000));

        sizer.update_risk_percentage(dec!(0.01));
        assert_eq!(sizer.risk_percentage(), dec!(0.01));

        // Test with updated values
        let quantity = sizer.calculate_quantity(dec!(1000));
        let expected = dec!(1000) / dec!(1000); // $1,000 / $1,000 = 1.0
        assert_eq!(quantity, expected);
    }
}