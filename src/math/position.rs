use crate::math::rounding::{div_ceil_u, div_floor_u, mul_div_i128};
use crate::state::Position;
use crate::types::{Side, TokenAmount, Usd};
/// - full close => all tokens
/// - partial:
///   - long => ceil(pos.size_tokens * size_delta_usd / pos.size_usd)
///   - short => floor(...)
pub fn size_delta_in_tokens(
    pos: &Position,
    size_delta_usd: Usd,
    is_full_close: bool,
) -> Result<TokenAmount, String> {
    if is_full_close || size_delta_usd == pos.size_usd {
        return Ok(pos.size_tokens);
    }
    if pos.size_usd <= 0 || pos.size_tokens <= 0 || size_delta_usd <= 0 {
        return Err("invalid_position_or_size_delta".into());
    }

    let n = pos
        .size_tokens
        .checked_mul(size_delta_usd)
        .ok_or("size_delta_mul_overflow")?;
    let t = match pos.key.side {
        Side::Long => div_ceil_u(n, pos.size_usd)?,
        Side::Short => div_floor_u(n, pos.size_usd)?,
    };
    Ok(t.max(0))
}

/// Proportional pending impact tokens (MVP, toward-zero):
pub fn proportional_pending_impact_tokens(
    pos: &Position,
    size_delta_usd: Usd,
) -> Result<TokenAmount, String> {
    if pos.size_usd <= 0 || size_delta_usd <= 0 {
        return Ok(0);
    }
    // toward-zero division is OK for MVP; GMX делает аналогичную пропорцию
    mul_div_i128(pos.pending_impact_tokens, size_delta_usd, pos.size_usd)
}
