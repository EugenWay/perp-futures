use crate::types::{MarketId, OraclePrices};

pub trait Oracle {
    fn validate_and_get_prices(&self, market_id: MarketId) -> Result<OraclePrices, String>;
}
