use crate::types::Usd;
use primitive_types::U256;

/// Generic fixed-point scale = 10^18.
fn fp_scale() -> i128 {
    10_i128.pow(18)
}

/// Protocol-level risk constraints.
#[derive(Clone, Copy, Debug)]
pub struct RiskCfg {
    /// Remaining positions below this size are treated as dust and should be fully closed.
    pub min_position_size_usd: Usd,

    /// Absolute minimum collateral (USD) required for a position to remain open.
    pub min_collateral_usd: Usd,

    /// Minimal collateral fraction (FP) required vs position notional.
    /// Example: max leverage 50x => min_collateral_factor = 1/50 = 0.02.
    pub min_collateral_factor_fp: i128,

    /// Fixed-point scale used by `min_collateral_factor_fp`.
    pub factor_scale: i128,
}

impl RiskCfg {
    /// MVP defaults
    pub fn mvp() -> Self {
        // Example: max leverage 50x => factor = 0.02 * 1e18
        let min_collateral_factor_fp = fp_scale() / 50;

        Self {
            min_position_size_usd: 10, // $10 dust threshold (tune as needed)
            min_collateral_usd: 5,     // $5 absolute floor (tune as needed)
            min_collateral_factor_fp,
            factor_scale: fp_scale(),
        }
    }
}

impl Default for RiskCfg {
    fn default() -> Self {
        Self::mvp()
    }
}
