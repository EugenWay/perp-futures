use crate::state::{MarketState, PoolBalances, Position};
use crate::types::{AssetId, MarketId, Timestamp, TokenAmount, Usd};

/// Internal scale for borrowing index.
/// Same idea as with funding: factor per 1 USD of position * SCALE.
const BORROW_INDEX_SCALE: i128 = 1_000_000; // 1e6

/// Result of borrowing settlement for a position.
#[derive(Debug, Clone, Copy)]
pub struct BorrowingDelta {
    /// Always >= 0 in a normal setup: borrowing is a pure cost.
    pub borrowing_fee_usd: Usd,
}

/// Service for borrowing logic:
/// - evolves market borrowing index over time;
/// - computes how much each position should pay.
pub trait BorrowingService {
    /// Update borrowing index for the market up to `now`,
    /// based on current utilization.
    fn update_index(&self, market: &mut MarketState, now: Timestamp);

    /// Compute borrowing fee for a position and update its snapshot.
    fn settle_position_borrowing(&self, market: &MarketState, pos: &mut Position)
    -> BorrowingDelta;
}

/// Basic implementation:
///
/// - utilization ≈ (oi_long + oi_short) / liquidity
/// - rate is a simple linear function of utilization:
///     rate_per_sec = base_rate + slope * utilization
#[derive(Default)]
pub struct BasicBorrowingService;

impl BasicBorrowingService {
    /// Compute utilization as a fixed-point in [0, 1] * BORROW_INDEX_SCALE.
    fn compute_utilization_fp(market: &MarketState) -> i128 {
        let borrowed = (market.oi_long_usd + market.oi_short_usd).max(0);
        let liquidity = market.liquidity_usd.max(0);

        if liquidity == 0 {
            return 0;
        }

        let ratio_fp = (borrowed as i128).saturating_mul(BORROW_INDEX_SCALE) / (liquidity as i128);

        // Cap at 1.0 in FP
        ratio_fp.min(BORROW_INDEX_SCALE)
    }
}

impl BorrowingService for BasicBorrowingService {
    fn update_index(&self, market: &mut MarketState, now: Timestamp) {
        if market.borrowing.last_updated_at == 0 {
            market.borrowing.last_updated_at = now;
            return;
        }
        if now <= market.borrowing.last_updated_at {
            return;
        }

        let dt: u64 = now - market.borrowing.last_updated_at;
        if dt == 0 {
            return;
        }

        // 1) Utilization in [0, 1] * SCALE
        let util_fp = Self::compute_utilization_fp(market);

        let borrowing = &mut market.borrowing;

        // 2) Simple linear rate:
        //
        //    rate_per_sec_fp = base_rate_fp + slope_fp * util
        //
        // Where:
        //   - base_rate_fp: minimal rate when utilization ~0.
        //   - slope_fp: how fast rate grows with utilization.
        //
        // Units: index units per second (same scale: BORROW_INDEX_SCALE).
        let base_rate_fp_per_sec: i128 = 5; // very small base rate (MVP)
        let slope_fp_per_sec: i128 = 20; // how much rate increases with util

        let rate_per_sec_fp = base_rate_fp_per_sec
            .saturating_add(slope_fp_per_sec.saturating_mul(util_fp) / BORROW_INDEX_SCALE);

        let delta_index_fp = rate_per_sec_fp.saturating_mul(dt as i128);

        borrowing.cumulative_factor = borrowing.cumulative_factor.saturating_add(delta_index_fp);
        borrowing.last_updated_at = now;
    }

    fn settle_position_borrowing(
        &self,
        market: &MarketState,
        pos: &mut Position,
    ) -> BorrowingDelta {
        let current_idx = market.borrowing.cumulative_factor;
        let prev_idx = pos.borrowing_index;

        let delta_idx = current_idx - prev_idx;
        if delta_idx <= 0 || pos.size_usd == 0 {
            pos.borrowing_index = current_idx;
            return BorrowingDelta {
                borrowing_fee_usd: 0,
            };
        }

        // borrowing_fee = sizeUsd * deltaIndex / SCALE
        let fee = (pos.size_usd as i128).saturating_mul(delta_idx) / BORROW_INDEX_SCALE;

        pos.borrowing_index = current_idx;

        BorrowingDelta {
            borrowing_fee_usd: fee,
        }
    }
}

/// Route borrowing fees (already converted to collateral tokens)
/// to the pool of (market, collateral_token).
pub fn apply_borrowing_fees_to_pool(
    pools: &mut PoolBalances,
    market_id: MarketId,
    collateral_token: AssetId,
    borrowing_tokens: TokenAmount,
) {
    if borrowing_tokens == 0 {
        return;
    }

    pools.add_fee_to_pool(market_id, collateral_token, borrowing_tokens);
}
