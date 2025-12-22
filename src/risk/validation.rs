use crate::risk::RiskCfg;
use crate::state::Position;
use crate::types::{OraclePrices, Order};
use crate::types::{TokenAmount, Usd};

/// Pre-check + normalization for decrease orders (no state mutation).
///
/// Returns:
/// - `final_size_delta_usd` (may be clamped to full close)
/// - `final_withdraw_tokens` (may be clamped to 0 or reduced)
/// - `is_full_close`
pub fn precheck_decrease_and_withdraw(
    pos: &Position,
    order: &Order,
    prices: &OraclePrices,
    risk: RiskCfg,
) -> Result<(Usd, TokenAmount, bool), String> {
    // Basic sanity checks (user-level + invariants).
    if pos.size_usd <= 0 || pos.size_tokens <= 0 {
        return Err("position_empty_or_corrupted".into());
    }
    if pos.collateral_amount < 0 {
        return Err("position_collateral_negative".into());
    }
    if prices.collateral_price_min <= 0 {
        return Err("invalid_collateral_price_min".into());
    }
    if risk.factor_scale <= 0 {
        return Err("invalid_factor_scale".into());
    }

    // 1) Normalize requested size delta.
    let mut size_delta_usd = order.size_delta_usd;
    if size_delta_usd <= 0 {
        return Err("size_delta_usd_must_be_positive".into());
    }
    if size_delta_usd > pos.size_usd {
        // MVP:
        size_delta_usd = pos.size_usd;
    }

    // 2) Determine full close.
    let mut is_full_close = size_delta_usd == pos.size_usd;

    // 3) Normalize withdraw request:
    // - on full close: force withdraw=0
    // - on partial close: allow but must be guarded
    if order.withdraw_collateral_amount < 0 {
        return Err("withdraw_collateral_amount_must_be_non_negative".into());
    }
    let mut withdraw_tokens: TokenAmount = if is_full_close {
        0
    } else {
        order.withdraw_collateral_amount
    };
    // User-level clamp: cannot withdraw more than collateral.
    if withdraw_tokens > pos.collateral_amount {
        // Option A strict: return Err("withdraw_exceeds_collateral".into());
        // Option B MVP: clamp
        withdraw_tokens = pos.collateral_amount;
    }

    // 4) Dust check: remaining size below min => force full close.
    let mut next_size_usd = pos
        .size_usd
        .checked_sub(size_delta_usd)
        .expect("size_delta_usd clamped to <= pos.size_usd");

    if next_size_usd != 0 && next_size_usd < risk.min_position_size_usd {
        size_delta_usd = pos.size_usd;
        withdraw_tokens = 0;
        is_full_close = true;
        next_size_usd = 0;
    }

    // 5) Conservative pre-check for partial close.
    // If unsafe with withdraw -> try withdraw=0.
    // If still unsafe -> force full close.
    if next_size_usd != 0 {
        let ok_with_withdraw = will_position_collateral_be_sufficient_pre(
            next_size_usd,
            pos.collateral_amount,
            withdraw_tokens,
            prices,
            risk,
        );

        if !ok_with_withdraw {
            withdraw_tokens = 0;

            let ok_without_withdraw = will_position_collateral_be_sufficient_pre(
                next_size_usd,
                pos.collateral_amount,
                withdraw_tokens,
                prices,
                risk,
            );

            if !ok_without_withdraw {
                size_delta_usd = pos.size_usd;
                withdraw_tokens = 0;
                is_full_close = true;
                next_size_usd = 0;
            }
        }
    } else {
        withdraw_tokens = 0;
        is_full_close = true;
    }

    Ok((size_delta_usd, withdraw_tokens, is_full_close))
}

/// Conservative "willPositionCollateralBeSufficient" PRE-check.
///
/// remainingCollateralUsd = (collateral - withdraw) * collateral_price_min
/// must satisfy:
/// 1) remainingCollateralUsd >= min_collateral_usd
/// 2) remainingCollateralUsd >= next_size_usd * min_collateral_factor
///
/// Returns false for user-level invalid requests.
/// Panics only on broken invariants (overflow, invalid prices).
pub fn will_position_collateral_be_sufficient_pre(
    next_size_usd: Usd,
    current_collateral_tokens: TokenAmount,
    withdraw_tokens: TokenAmount,
    prices: &OraclePrices,
    risk: RiskCfg,
) -> bool {
    // User-level: cannot withdraw more than available collateral.
    if withdraw_tokens > current_collateral_tokens {
        return false;
    }

    // Invariant: oracle must provide positive collateral_price_min.
    if prices.collateral_price_min <= 0 {
        panic!("oracle invariant violated: collateral_price_min <= 0");
    }

    let next_collateral_tokens = current_collateral_tokens
        .checked_sub(withdraw_tokens)
        .expect("withdraw_tokens <= collateral_tokens enforced above");

    let remaining_collateral_usd = next_collateral_tokens
        .checked_mul(prices.collateral_price_min)
        .expect("remaining_collateral_usd overflow");

    if remaining_collateral_usd < risk.min_collateral_usd {
        return false;
    }

    // minCollateralUsdForLeverage = next_size_usd * factor / scale
    let min_for_leverage = next_size_usd
        .checked_mul(risk.min_collateral_factor_fp)
        .expect("min_for_leverage mul overflow")
        .checked_div(risk.factor_scale)
        .expect("factor_scale must be > 0");

    remaining_collateral_usd >= min_for_leverage
}

/// Post-check after settlement (fees, realized PnL, collateral changes).
///
/// Use this after you compute the new `pos` values (or right before persisting them).
pub fn postcheck_remaining_position(
    pos_after: &Position,
    prices: &OraclePrices,
    risk: RiskCfg,
) -> Result<(), String> {
    if pos_after.size_usd == 0 {
        return Ok(()); // closed is always fine
    }
    if pos_after.size_usd < 0 || pos_after.size_tokens < 0 || pos_after.collateral_amount < 0 {
        return Err("position_negative_values_after_settlement".into());
    }
    if prices.collateral_price_min <= 0 {
        return Err("invalid_collateral_price_min".into());
    }

    let remaining_collateral_usd = pos_after
        .collateral_amount
        .checked_mul(prices.collateral_price_min)
        .ok_or_else(|| "collateral_usd_overflow".to_string())?;

    if remaining_collateral_usd < risk.min_collateral_usd {
        return Err("remaining_collateral_below_min".into());
    }

    let min_for_leverage = pos_after
        .size_usd
        .checked_mul(risk.min_collateral_factor_fp)
        .ok_or_else(|| "min_for_leverage_overflow".to_string())?
        .checked_div(risk.factor_scale)
        .ok_or_else(|| "invalid_factor_scale".to_string())?;

    if remaining_collateral_usd < min_for_leverage {
        return Err("remaining_position_exceeds_max_leverage".into());
    }

    Ok(())
}

/// Future: liquidation predicate (placeholder).
///
/// - remainingCollateralUsd = collateralUsd + pnlUsd + priceImpactUsd - feesUsd
/// - remainingCollateralUsd <= 0 or < minCollateralUsd or < minCollateralUsdForLeverage
///
/// For MVP you can keep this unimplemented until PnL + priceImpact on decrease is wired.
#[allow(dead_code)]
pub fn is_position_liquidatable_future_placeholder() {
    // TODO
}
