// src/services/funding_step.rs

use crate::services::FundingService;
use crate::state::{Claimables, MarketState, Position};
use crate::types::{AssetId, OraclePrices, Side, TokenAmount, Usd};

/// Result of applying funding for a single position on a single step.
#[derive(Debug, Clone, Copy)]
pub struct FundingStep {
    /// How much funding this position must pay in USD (payer side).
    /// Always >= 0.
    pub cost_usd: Usd,
}

/// Apply funding for a single position:
///  - calls FundingService::settle_position_funding (updates pos.funding_index),
///  - if the position is on the payer side => returns positive cost_usd,
///  - if on receiver side => mints Claimables in collateral token and returns cost_usd = 0.
pub fn apply_funding_step<F: FundingService>(
    funding_svc: &F,
    market: &MarketState,
    pos: &mut Position,
    claimables: &mut Claimables,
    prices: &OraclePrices,
) -> Result<FundingStep, String> {
    let delta = funding_svc.settle_position_funding(market, pos);
    let fee_usd = delta.funding_fee_usd;

    if fee_usd == 0 {
        return Ok(FundingStep { cost_usd: 0 });
    }

    if fee_usd > 0 {
        // Payer side: the user pays funding.
        Ok(FundingStep { cost_usd: fee_usd })
    } else {
        // Receiver side: the user earns funding.
        if prices.collateral_price_min <= 0 {
            return Err("invalid_collateral_price_min_for_funding".into());
        }

        let reward_usd: Usd = -fee_usd;
        let reward_tokens: TokenAmount = reward_usd / prices.collateral_price_min;

        if reward_tokens > 0 {
            // We store claimables in the collateral token of the position.
            claimables.add_funding(pos.key.account, pos.key.collateral_token, reward_tokens);
        }

        // No cost for the position (only reward recorded in Claimables).
        Ok(FundingStep { cost_usd: 0 })
    }
}
