// src/oracle.rs

use crate::types::{MarketId, OraclePrices};

pub trait Oracle {
    fn validate_and_get_prices(
        &self,
        market_id: MarketId,
        // тут потом добавишь подписи/пакеты цен
    ) -> Result<OraclePrices, String>;
}
