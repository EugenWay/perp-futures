// src/types.rs
use std::hash::Hash;

pub type Timestamp = u64;

pub type Usd = i128;

pub type TokenAmount = i128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct MarketId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AssetId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OrderId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AccountId(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Long,
    Short,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderType {
    Increase,
    Decrease,
    Liquidation,
}

#[derive(Clone, Copy, Debug)]
pub struct OraclePrices {
    pub index_price_min: Usd,
    pub index_price_max: Usd,
    pub collateral_price_min: Usd,
    pub collateral_price_max: Usd,
}

#[derive(Clone, Debug)]
pub struct Order {
    pub account: AccountId,
    pub market_id: MarketId,
    pub collateral_token: AssetId,
    pub side: Side,
    pub order_type: OrderType,
    pub collateral_delta_tokens: TokenAmount,
    pub size_delta_usd: Usd,
    /// withdraw collateral tokens while partially closing.
    /// This is independent from size_delta_usd and can increase leverage if not guarded.
    pub withdraw_collateral_amount: TokenAmount,

    /// Target leverage X for this step, e.g. 5 means 5x.
    pub target_leverage_x: i64,

    pub created_at: Timestamp,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
}
