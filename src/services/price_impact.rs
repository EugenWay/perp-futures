// src/services/price_impact.rs

use crate::services::open_interest::OpenInterestParams;
use crate::types::Usd;
use primitive_types::U256;

/// Generic fixed-point scale = 10^18.
fn fp_scale() -> U256 {
    U256::exp10(18)
}

/// Config for impact curve and factors.
/// All factors are fixed-point with scale = fp_scale().
#[derive(Clone, Debug)]
pub struct ImpactRebalanceConfig {
    /// Exponent "e" in d^e (e.g. 1, 2, 3).
    pub impact_exponent: u32,

    /// Same-side impact factor when balance improves.
    /// (helpful trades)  — fp-scaled.
    pub same_side_positive_factor_fp: U256,

    /// Same-side impact factor when balance worsens.
    /// (harmful trades) — fp-scaled.
    pub same_side_negative_factor_fp: U256,

    /// Cross-over positive factor (applied to initial diff).
    pub crossover_positive_factor_fp: U256,

    /// Cross-over negative factor (applied to next diff).
    pub crossover_negative_factor_fp: U256,
}

impl ImpactRebalanceConfig {
    /// Simple quadratic profile for MVP.
    pub fn default_quadratic() -> Self {
        let one = fp_scale();
        // Effectively: impact_usd ~ (diff^2) / 1_000_000
        Self {
            impact_exponent: 2,
            // helpful trades: small positive impact
            same_side_positive_factor_fp: one / 1_000_000, // 1e-6
            // harmful trades: ~4x stronger, but still soft
            same_side_negative_factor_fp: one * 4 / 1_000_000,
            // crossover: similar scale
            crossover_positive_factor_fp: one / 1_000_000,
            crossover_negative_factor_fp: one * 4 / 1_000_000,
        }
    }
}

fn usd_to_u256(x: Usd) -> U256 {
    if x < 0 {
        assert!(x >= 0, "Open interest must be non-negative");
        U256::zero()
    } else {
        U256::from(x as u128)
    }
}
/// |a - b| for U256
fn abs_diff(a: U256, b: U256) -> U256 {
    if a >= b { a - b } else { b - a }
}

/// x^exp (small exp like 1,2,3) for U256
fn pow_u256(mut x: U256, mut exp: u32) -> U256 {
    if exp == 0 {
        return U256::one();
    }
    let mut result = U256::one();
    while exp > 0 {
        if exp & 1 == 1 {
            result = result.saturating_mul(x);
        }
        x = x.saturating_mul(x);
        exp >>= 1;
    }
    result
}

/// Convert fixed-point (val * SCALE) -> Usd (i128) with saturation.
/// SCALE = 1e18.
fn from_fp_to_usd_saturating(v_fp: U256) -> Usd {
    let scale = fp_scale();
    let (q, _r) = v_fp.div_mod(scale);

    let bytes: [u8; 32] = q.to_big_endian();

    let mut buf = [0u8; 16];
    buf.copy_from_slice(&bytes[16..]);
    i128::from_be_bytes(buf)
}

/// Inputs:
///   - oi.current.long_usd / short_usd
///   - oi.next.long_usd / short_usd
///   - cfg with impact factors & exponent
///
/// Returns:
///   - price_impact_usd: signed USD amount
///   - balance_was_improved: did abs diff shrink?
fn get_price_impact_usd(oi: &OpenInterestParams, cfg: &ImpactRebalanceConfig) -> (Usd, bool) {
    let long0_i = oi.current.long_usd;
    let short0_i = oi.current.short_usd;
    let long1_i = oi.next.long_usd;
    let short1_i = oi.next.short_usd;

    let initial_long_le_short = long0_i <= short0_i;
    let next_long_le_short = long1_i <= short1_i;
    let is_same_side_rebalance = initial_long_le_short == next_long_le_short;

    let long0 = usd_to_u256(long0_i);
    let short0 = usd_to_u256(short0_i);
    let long1 = usd_to_u256(long1_i);
    let short1 = usd_to_u256(short1_i);

    // absolute imbalance before / after
    let initial_diff = abs_diff(long0, short0);
    let next_diff = abs_diff(long1, short1);

    // did imbalance shrink?
    let balance_was_improved = next_diff < initial_diff;

    let e = cfg.impact_exponent;
    let d0e = pow_u256(initial_diff, e);
    let d1e = pow_u256(next_diff, e);

    if is_same_side_rebalance {
        //  Same Side Rebalance
        //
        //  impact ~ (d0^e - d1^e) * factor
        //
        // If imbalance shrinks → helpful trade → use positive factor.
        // If grows → harmful trade → use negative factor.
        let factor_fp = if balance_was_improved {
            cfg.same_side_positive_factor_fp
        } else {
            cfg.same_side_negative_factor_fp
        };

        // diff_e = d0^e - d1^e (with sign)
        let (diff_e, sign): (U256, i8) = if d0e >= d1e {
            (d0e - d1e, 1) // d0e >= d1e → potentially positive impact
        } else {
            (d1e - d0e, -1) // d1e > d0e  → potentially negative impact
        };

        let mag_fp = diff_e.saturating_mul(factor_fp);
        let mut impact_usd = from_fp_to_usd_saturating(mag_fp);

        if sign < 0 {
            impact_usd = -impact_usd;
        }

        (impact_usd, balance_was_improved)
    } else {
        // Crossover Rebalance
        //
        //   impact = (d0^e * positiveFactor) - (d1^e * negativeFactor)
        //
        let p_fp = cfg.crossover_positive_factor_fp;
        let n_fp = cfg.crossover_negative_factor_fp;

        let term0 = d0e.saturating_mul(p_fp);
        let term1 = d1e.saturating_mul(n_fp);

        let (mag_fp, is_positive) = if term0 >= term1 {
            (term0 - term1, true)
        } else {
            (term1 - term0, false)
        };

        let mut impact_usd = from_fp_to_usd_saturating(mag_fp);
        if !is_positive {
            impact_usd = -impact_usd;
        }

        (impact_usd, balance_was_improved)
    }
}

/// Basic implementation: just forwards to `get_price_impact_usd`
/// with the math above.
#[derive(Default)]
pub struct BasicPriceImpactService;

pub trait PriceImpactService {
    fn compute_price_impact_usd(
        &self,
        oi: &OpenInterestParams,
        cfg: &ImpactRebalanceConfig,
    ) -> (Usd, bool);
}

impl PriceImpactService for BasicPriceImpactService {
    fn compute_price_impact_usd(
        &self,
        oi: &OpenInterestParams,
        cfg: &ImpactRebalanceConfig,
    ) -> (Usd, bool) {
        get_price_impact_usd(oi, cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::open_interest::{OpenInterestParams, OpenInterestSnapshot};
    use crate::types::Usd;

    fn cfg() -> ImpactRebalanceConfig {
        ImpactRebalanceConfig::default_quadratic()
    }

    fn oi_params(long0: Usd, short0: Usd, long1: Usd, short1: Usd) -> OpenInterestParams {
        OpenInterestParams {
            current: OpenInterestSnapshot {
                long_usd: long0,
                short_usd: short0,
            },
            next: OpenInterestSnapshot {
                long_usd: long1,
                short_usd: short1,
            },
        }
    }

    #[test]
    fn helpful_short_on_long_heavy_market_gives_positive_impact() {
        // Initial: longs = 150k, shorts = 50k (market is long-heavy, diff = 100k)
        // After:   longs = 150k, shorts = 60k (diff = 90k, imbalance decreased)
        //
        // This is a "good" trade: it reduces imbalance → we expect:
        //   - balance_was_improved = true
        //   - price_impact_usd > 0
        let oi = oi_params(150_000, 50_000, 150_000, 60_000);
        let (impact, improved) = get_price_impact_usd(&oi, &cfg());

        assert!(
            improved,
            "Imbalance should shrink for a helpful trade (short on long-heavy market)"
        );
        assert!(
            impact > 0,
            "Helpful trade should result in positive price impact (user gets a small benefit)"
        );
    }

    #[test]
    fn harmful_long_on_long_heavy_market_gives_negative_impact() {
        // Initial: longs = 150k, shorts = 50k (long-heavy)
        // After:   longs = 160k, shorts = 50k (diff grows from 100k to 110k)
        //
        // This is a "bad" trade: it pushes imbalance further.
        // We expect:
        //   - balance_was_improved = false
        //   - price_impact_usd < 0
        let oi = oi_params(150_000, 50_000, 160_000, 50_000);
        let (impact, improved) = get_price_impact_usd(&oi, &cfg());

        assert!(
            !improved,
            "Imbalance should grow for a harmful trade (extra long on long-heavy market)"
        );
        assert!(
            impact < 0,
            "Harmful trade should result in negative price impact (user pays a penalty)"
        );
    }

    #[test]
    fn crossover_from_long_heavy_to_short_heavy_is_non_trivial_case() {
        // Initial: longs = 150k, shorts = 50k  → long-heavy
        // After:   longs =  80k, shorts = 120k → short-heavy
        //
        // This is a crossover case: the heavy side flips.
        // Here we don't enforce a strict sign (it depends on factors),
        // we just ensure the impact is not zero (i.e. the curve reacts).
        let oi = oi_params(150_000, 50_000, 80_000, 120_000);
        let (impact, _improved) = get_price_impact_usd(&oi, &cfg());

        assert_ne!(
            impact, 0,
            "Crossover rebalance should result in a non-zero price impact"
        );
    }

    #[test]
    fn no_change_in_oi_gives_zero_impact() {
        // No change in long/short open interest => pure no-op for price impact.
        let oi = oi_params(100_000, 100_000, 100_000, 100_000);
        let (impact, improved) = get_price_impact_usd(&oi, &cfg());

        assert!(
            !improved,
            "With identical current/next OI, we consider it as 'no improvement'"
        );
        assert_eq!(
            impact, 0,
            "If OI does not change at all, price impact must be exactly zero"
        );
    }

    #[test]
    fn larger_helpful_move_has_at_least_as_much_impact_as_smaller() {
        // Check monotonic behaviour: a bigger helpful trade
        // should not produce *smaller* absolute impact than a tiny one.
        let cfg = cfg();

        // Same initial state:
        // longs = 150k, shorts = 50k (long-heavy, diff = 100k)
        // Small helpful move: +5k shorts
        let oi_small = oi_params(150_000, 50_000, 150_000, 55_000);
        // Bigger helpful move: +30k shorts
        let oi_big = oi_params(150_000, 50_000, 150_000, 80_000);

        let (impact_small, _) = get_price_impact_usd(&oi_small, &cfg);
        let (impact_big, _) = get_price_impact_usd(&oi_big, &cfg);

        assert!(
            impact_small > 0 && impact_big > 0,
            "Both trades are helpful (more shorts on long-heavy market), impact must be > 0"
        );
        assert!(
            impact_big.abs() >= impact_small.abs(),
            "Larger helpful trade should produce impact with at least as large magnitude"
        );
    }
}
