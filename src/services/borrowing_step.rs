use crate::services::BorrowingService;
use crate::state::{MarketState, Position};
use crate::types::Usd;

/// Result of applying borrowing for a single position on a single step.
#[derive(Debug, Clone, Copy)]
pub struct BorrowingStep {
    /// How much borrowing cost this position must pay in USD.
    /// Always >= 0.
    pub cost_usd: Usd,
}

/// Apply borrowing logic to a single position:
///
///  - calls `borrowing_svc.settle_position_borrowing(market, pos)`
///    which updates borrowing index snapshot inside the position;
///  - interprets the returned `borrowing_fee_usd` as a pure cost (payer-only)
///    and returns it as `BorrowingStep { cost_usd }`.
///
/// NOTE:
///  - In GMX-style funding/borrowing, borrowing is usually always a cost
///    (no "receiver" side), so there are no Claimables here.
pub fn apply_borrowing_step<B: BorrowingService>(
    borrowing_svc: &B,
    market: &MarketState,
    pos: &mut Position,
) -> BorrowingStep {
    let delta = borrowing_svc.settle_position_borrowing(market, pos);
    let fee: Usd = delta.borrowing_fee_usd;

    // Borrowing is expected to be a cost. If your implementation can
    // produce negative values, we clip them to zero here.
    BorrowingStep {
        cost_usd: fee.max(0),
    }
}
