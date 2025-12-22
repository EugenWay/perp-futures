use crate::math::rounding::{div_ceil_u, div_floor_u, mul_div_i128};
use crate::state::Position;
use crate::types::{OraclePrices, Side, TokenAmount, Usd};
fn pick_price_for_pnl(side: Side, prices: &OraclePrices) -> Result<Usd, String> {
    let p = match side {
        Side::Long => prices.index_price_min,
        Side::Short => prices.index_price_max,
    };
    if p <= 0 {
        return Err("invalid_pnl_price".into());
    }
    Ok(p)
}

/// Total position PnL in USD (MVP):
pub fn total_position_pnl_usd(pos: &Position, prices: &OraclePrices) -> Result<Usd, String> {
    let px = pick_price_for_pnl(pos.key.side, prices)?;
    let value = pos
        .size_tokens
        .checked_mul(px)
        .ok_or("pnl_value_overflow")?;

    let pnl = match pos.key.side {
        Side::Long => value - pos.size_usd,
        Side::Short => pos.size_usd - value,
    };
    Ok(pnl)
}

/// Realized PnL for partial close
pub fn realized_pnl_usd(
    total_pnl_usd: Usd,
    size_delta_tokens: TokenAmount,
    pos_size_tokens: TokenAmount,
) -> Result<Usd, String> {
    if pos_size_tokens <= 0 {
        return Err("invalid_pos_size_tokens".into());
    }
    mul_div_i128(total_pnl_usd, size_delta_tokens, pos_size_tokens)
}

/// Convert +/- pnlUsd to collateral tokens:
/// +PnL: floor(pnlUsd / collateral_price_max) (min payout tokens)
/// -PnL: ceil(abs(pnlUsd) / collateral_price_min) (max cost tokens)
pub fn pnl_usd_to_collateral_tokens(
    pnl_usd: Usd,
    prices: &OraclePrices,
) -> Result<TokenAmount, String> {
    if pnl_usd == 0 {
        return Ok(0);
    }

    if pnl_usd > 0 {
        let p = prices.collateral_price_max;
        if p <= 0 {
            return Err("invalid_collateral_price_max".into());
        }
        Ok(div_floor_u(pnl_usd, p)?)
    } else {
        let p = prices.collateral_price_min;
        if p <= 0 {
            return Err("invalid_collateral_price_min".into());
        }
        let abs = -pnl_usd;
        Ok(-div_ceil_u(abs, p)?)
    }
}
