pub fn div_ceil_u(a: i128, b: i128) -> Result<i128, String> {
    if a < 0 || b <= 0 {
        return Err("div_ceil_invalid".into());
    }
    let q = a / b;
    let r = a % b;
    Ok(if r == 0 { q } else { q + 1 })
}

pub fn div_floor_u(a: i128, b: i128) -> Result<i128, String> {
    if a < 0 || b <= 0 {
        return Err("div_floor_invalid".into());
    }
    Ok(a / b)
}

pub fn mul_div_i128(a: i128, b: i128, denom: i128) -> Result<i128, String> {
    if denom == 0 {
        return Err("mul_div_div0".into());
    }
    Ok(a.checked_mul(b)
        .ok_or("mul_overflow")?
        .checked_div(denom)
        .ok_or("div_overflow")?
        .into())
}
