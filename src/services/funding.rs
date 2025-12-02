use crate::state::{MarketState, Position};
use crate::types::{Side, Timestamp, Usd};

/// Internal scale for funding index.
/// Index is stored as "funding USD per 1 USD of position * SCALE".
const FUNDING_INDEX_SCALE: i128 = 1_000_000; // 1e6

/// Result of funding settlement for a single position.
#[derive(Debug, Clone, Copy)]
pub struct FundingDelta {
    /// Positive value means "user pays", negative — "user receives".
    pub funding_fee_usd: Usd,
}

/// Funding service: responsible for
/// - evolving market funding indices over time;
/// - computing per-position funding deltas based on snapshots.
pub trait FundingService {
    /// Update market funding indices up to `now`, based on current OI imbalance.
    fn update_indices(&self, market: &mut MarketState, now: Timestamp);

    /// Compute funding delta for a given position (using market indices)
    /// and update the position snapshot to the latest index.
    ///
    /// Returns how much funding this position should pay (positive)
    /// or receive (negative) in USD.
    fn settle_position_funding(&self, market: &MarketState, pos: &mut Position) -> FundingDelta;
}

/// Basic implementation:
///
/// - Uses a very simple rule:
///     * If longs > shorts → longs pay a fixed rate to shorts.
///     * If shorts > longs → shorts pay a fixed rate to longs.
/// - Rate depends on imbalance **sign**, not magnitude (MVP).
#[derive(Default)]
pub struct BasicFundingService;

fn current_index_for_side(market: &MarketState, side: Side) -> i128 {
    match side {
        Side::Long => market.funding.cumulative_index_long,
        Side::Short => market.funding.cumulative_index_short,
    }
}

impl FundingService for BasicFundingService {
    fn update_indices(&self, market: &mut MarketState, now: Timestamp) {
        let funding = &mut market.funding;

        // 1) First-time init or no time passed.
        if funding.last_updated_at == 0 {
            funding.last_updated_at = now;
            return;
        }
        if now <= funding.last_updated_at {
            return;
        }

        let dt: u64 = now - funding.last_updated_at;
        if dt == 0 {
            return;
        }

        // 2) Read current OI.
        let long_oi = market.oi_long_usd.max(0);
        let short_oi = market.oi_short_usd.max(0);
        let total_oi = long_oi + short_oi;

        // If there is no open interest at all, funding does not move.
        if total_oi == 0 {
            funding.last_updated_at = now;
            return;
        }

        // >0 => long-heavy, <0 => short-heavy
        let imbalance = long_oi - short_oi;

        // 3) Very simple rule for MVP:
        //
        //    - If market is long-heavy → longs pay shorts at a fixed rate.
        //    - If short-heavy → shorts pay longs.
        //
        // rate_abs_fp is "index units per second", in FUNDING_INDEX_SCALE.
        //
        // Example: 1e-8 per second ≈ 0.0000864 per day (0.00864%).
        // TODO: can tune this later.
        let rate_abs_fp_per_sec: i128 = 10; // extremely small for MVP

        let delta_index_fp = rate_abs_fp_per_sec * dt as i128;

        if imbalance > 0 {
            // Long-heavy → longs pay, shorts receive.
            funding.cumulative_index_long =
                funding.cumulative_index_long.saturating_add(delta_index_fp);
            funding.cumulative_index_short = funding
                .cumulative_index_short
                .saturating_sub(delta_index_fp);
        } else if imbalance < 0 {
            // Short-heavy → shorts pay, longs receive.
            funding.cumulative_index_long =
                funding.cumulative_index_long.saturating_sub(delta_index_fp);
            funding.cumulative_index_short = funding
                .cumulative_index_short
                .saturating_add(delta_index_fp);
        }

        funding.last_updated_at = now;
    }

    fn settle_position_funding(&self, market: &MarketState, pos: &mut Position) -> FundingDelta {
        // 1) Choose market index for position side (long/short).
        let current_idx = current_index_for_side(market, pos.key.side);
        let prev_idx = pos.funding_index;

        let delta_idx = current_idx - prev_idx;
        if delta_idx == 0 || pos.size_usd == 0 {
            // Nothing to settle.
            pos.funding_index = current_idx;
            return FundingDelta { funding_fee_usd: 0 };
        }

        // 2) funding_fee_usd = sizeUsd * deltaIndex / SCALE
        //
        // Convention:
        //   - Positive funding_fee_usd → user pays.
        //   - Negative funding_fee_usd → user receives.
        //
        // Since we made payers' index go UP, receivers' index go DOWN,
        // the formula below automatically gives the right sign:
        let fee = (pos.size_usd as i128).saturating_mul(delta_idx) / FUNDING_INDEX_SCALE;

        // 3) Update position snapshot to the latest index.
        pos.funding_index = current_idx;

        FundingDelta {
            funding_fee_usd: fee,
        }
    }
}
