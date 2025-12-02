// src/services/pricing.rs

use crate::services::open_interest::OpenInterestParams;
use crate::services::price_impact::{ImpactRebalanceConfig, PriceImpactService};
use crate::types::{OraclePrices, Side, TokenAmount, Usd};

#[derive(Debug)]
pub enum PricingError {
    ZeroSizeDelta,
    PriceImpactLargerThanOrderSize {
        price_impact_usd: Usd,
        size_delta_usd: Usd,
    },
    ZeroSizeTokensAfterImpact,
}

/// Input params for execution price calculation on increase.
pub struct ExecutionPriceIncreaseParams<'a> {
    /// Long / short OI before and after the action.
    pub oi: &'a OpenInterestParams,
    /// Market config for impact exponents and factors.
    pub impact_cfg: &'a ImpactRebalanceConfig,
    /// Side (long / short).
    pub side: Side,
    /// Requested size delta in USD.
    pub size_delta_usd: Usd,
    /// Oracle min / max prices.
    pub prices: OraclePrices,
}

#[derive(Debug, Clone)]
pub struct ExecutionPriceIncreaseResult {
    pub price_impact_usd: Usd,
    pub price_impact_amount_tokens: TokenAmount,
    pub base_size_delta_tokens: TokenAmount,
    pub size_delta_tokens: TokenAmount,
    pub execution_price: Usd,
    pub balance_was_improved: bool,
}

/// High-level trait for pricing logic.
pub trait PricingService {
    fn get_execution_price_for_increase(
        &self,
        price_impact: &dyn PriceImpactService,
        params: ExecutionPriceIncreaseParams,
    ) -> Result<ExecutionPriceIncreaseResult, PricingError>;
}

/// Basic implementation that uses a PriceImpactService inside.
#[derive(Default)]
pub struct BasicPricingService;

impl PricingService for BasicPricingService {
    fn get_execution_price_for_increase(
        &self,
        price_impact: &dyn PriceImpactService,
        params: ExecutionPriceIncreaseParams,
    ) -> Result<ExecutionPriceIncreaseResult, PricingError> {
        let ExecutionPriceIncreaseParams {
            oi,
            impact_cfg,
            side,
            size_delta_usd,
            prices,
        } = params;

        // 0) trivial branch: sizeDeltaUsd == 0
        if size_delta_usd == 0 {
            // No impact, just pick index price.
            let execution_price = match side {
                Side::Long => prices.index_price_max,
                Side::Short => prices.index_price_min,
            };

            return Ok(ExecutionPriceIncreaseResult {
                price_impact_usd: 0,
                price_impact_amount_tokens: 0,
                base_size_delta_tokens: 0,
                size_delta_tokens: 0,
                execution_price,
                balance_was_improved: false,
            });
        }

        // 1) compute priceImpactUsd from OI before/after
        let (price_impact_usd, balance_was_improved) =
            price_impact.compute_price_impact_usd(oi, impact_cfg);

        // 2) convert priceImpactUsd -> priceImpactAmount (tokens) ---
        //
        //  - if priceImpactUsd > 0:
        //        use indexPrice.max and round down (minimize bonus tokens)
        //  - if priceImpactUsd < 0:
        //        use indexPrice.min and round UP (maximize penalty tokens)
        let mut price_impact_amount_tokens: TokenAmount = 0;

        if price_impact_usd > 0 {
            let p_max = prices.index_price_max;
            if p_max > 0 {
                price_impact_amount_tokens = price_impact_usd / p_max;
            }
        } else if price_impact_usd < 0 {
            let p_min = prices.index_price_min;
            if p_min > 0 {
                let abs = -price_impact_usd;
                let q = abs / p_min;
                let r = abs % p_min;
                let ceil = if r == 0 { q } else { q + 1 };
                price_impact_amount_tokens = -ceil;
            }
        }

        // 3) baseSizeDeltaInTokens (without price impact)
        //
        // For long:
        //   - use indexPrice.max, round DOWN
        //
        // For short:
        //   - use indexPrice.min, round UP
        let base_size_delta_tokens: TokenAmount = match side {
            Side::Long => {
                let p_max = prices.index_price_max;
                if p_max > 0 {
                    size_delta_usd / p_max
                } else {
                    return Err(PricingError::ZeroSizeDelta);
                }
            }
            Side::Short => {
                let p_min = prices.index_price_min;
                if p_min > 0 {
                    let q = size_delta_usd / p_min;
                    let r = size_delta_usd % p_min;
                    if r == 0 { q } else { q + 1 }
                } else {
                    return Err(PricingError::ZeroSizeDelta);
                }
            }
        };

        //  4) total sizeDeltaInTokens including impact ---
        //
        //   if long:
        //      sizeDeltaInTokens = base + priceImpactAmount
        //   if short:
        //      sizeDeltaInTokens = base - priceImpactAmount
        let size_delta_tokens: TokenAmount = match side {
            Side::Long => base_size_delta_tokens + price_impact_amount_tokens,
            Side::Short => base_size_delta_tokens - price_impact_amount_tokens,
        };

        if size_delta_tokens < 0 {
            return Err(PricingError::PriceImpactLargerThanOrderSize {
                price_impact_usd,
                size_delta_usd,
            });
        }

        if size_delta_tokens == 0 {
            return Err(PricingError::ZeroSizeTokensAfterImpact);
        }

        // 5) executionPrice = sizeDeltaUsd / sizeDeltaInTokens ---
        //
        // TODO: acceptablePrice
        let execution_price: Usd = size_delta_usd / size_delta_tokens;

        Ok(ExecutionPriceIncreaseResult {
            price_impact_usd,
            price_impact_amount_tokens,
            base_size_delta_tokens,
            size_delta_tokens,
            execution_price,
            balance_was_improved,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::open_interest::{OpenInterestParams, OpenInterestSnapshot};
    use crate::services::price_impact::{BasicPriceImpactService, ImpactRebalanceConfig};
    use crate::types::{OraclePrices, Side, Usd};

    fn mk_oi(long0: Usd, short0: Usd, long1: Usd, short1: Usd) -> OpenInterestParams {
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

    fn mk_prices(min: Usd, max: Usd) -> OraclePrices {
        OraclePrices {
            index_price_min: min,
            index_price_max: max,
            collateral_price_min: min,
            collateral_price_max: max,
        }
    }

    #[test]
    fn zero_size_delta_returns_zero_impact_and_uses_oracle_price() {
        let pricing = BasicPricingService::default();
        let impact = BasicPriceImpactService::default();

        let oi = mk_oi(100_000, 100_000, 100_000, 100_000);
        let cfg = ImpactRebalanceConfig::default_quadratic();
        let prices = mk_prices(1_000, 1_100);

        let res = pricing
            .get_execution_price_for_increase(
                &impact,
                ExecutionPriceIncreaseParams {
                    oi: &oi,
                    impact_cfg: &cfg,
                    side: Side::Long,
                    size_delta_usd: 0,
                    prices,
                },
            )
            .expect("pricing should succeed for zero size");

        // With zero size, we expect no impact and no tokens,
        // only a "reference" execution price taken from oracle.
        assert_eq!(res.price_impact_usd, 0);
        assert_eq!(res.price_impact_amount_tokens, 0);
        assert_eq!(res.base_size_delta_tokens, 0);
        assert_eq!(res.size_delta_tokens, 0);
        // For longs we pick index_price_max
        assert_eq!(res.execution_price, prices.index_price_max);
    }

    #[test]
    fn helpful_long_trade_gets_more_tokens_and_better_price_than_base() {
        let pricing = BasicPricingService::default();
        let impact = BasicPriceImpactService::default();

        // Market is short-heavy (shorts > longs), opening a long helps rebalance.
        // So this long trade is "helpful".
        let oi = mk_oi(50_000, 150_000, 60_000, 150_000);
        let cfg = ImpactRebalanceConfig::default_quadratic();
        // Use same min/max to eliminate rounding differences
        let prices = mk_prices(1_000, 1_000);

        let size_delta_usd: Usd = 10_000;

        let res = pricing
            .get_execution_price_for_increase(
                &impact,
                ExecutionPriceIncreaseParams {
                    oi: &oi,
                    impact_cfg: &cfg,
                    side: Side::Long,
                    size_delta_usd,
                    prices,
                },
            )
            .expect("pricing should succeed");

        // Base tokens without any price impact:
        let expected_base = size_delta_usd / prices.index_price_max;
        assert_eq!(
            res.base_size_delta_tokens, expected_base,
            "Base tokens must match simple size / max_price division"
        );

        // Helpful trade should not penalize the user.
        assert!(
            res.price_impact_usd >= 0,
            "Helpful long trade should have non-negative impact (ideally positive)"
        );
        assert!(
            res.size_delta_tokens >= res.base_size_delta_tokens,
            "Helpful long should receive at least as many tokens as the base amount"
        );

        // If we get more tokens for same USD size, execution price goes down (better).
        let base_price = prices.index_price_max;
        assert!(
            res.execution_price <= base_price,
            "Execution price for a helpful long should be no worse (<=) than the base price"
        );
    }

    #[test]
    fn harmful_long_trade_gets_fewer_tokens_and_worse_price_than_base() {
        let pricing = BasicPricingService::default();
        let impact = BasicPriceImpactService::default();

        // Slightly long-heavy market.
        // Initial: longs = 100_500, shorts = 100_000 (diff = 500)
        // After:   longs = 101_000, shorts = 100_000 (diff = 1_000, imbalance increased)
        //
        // This is still a "harmful" trade (it makes the imbalance worse),
        // but the imbalance is small enough that the negative impact
        // does not exceed the order size.
        let oi = mk_oi(100_500, 100_000, 101_000, 100_000);

        let cfg = ImpactRebalanceConfig::default_quadratic();
        let prices = mk_prices(1_000, 1_000);
        let size_delta_usd: Usd = 10_000;

        let res = pricing
            .get_execution_price_for_increase(
                &impact,
                ExecutionPriceIncreaseParams {
                    oi: &oi,
                    impact_cfg: &cfg,
                    side: Side::Long,
                    size_delta_usd,
                    prices,
                },
            )
            .expect("pricing should succeed for harmful long trade");

        let expected_base = size_delta_usd / prices.index_price_max;
        assert_eq!(
            res.base_size_delta_tokens, expected_base,
            "Base tokens must be computed as size / max_price for longs"
        );

        // Harmful trade: we still expect a penalty,
        // but not so large that it wipes out the entire position.
        assert!(
            res.price_impact_usd <= 0,
            "Harmful long trade should have non-positive impact (ideally negative)"
        );
        assert!(
            res.size_delta_tokens <= res.base_size_delta_tokens,
            "Harmful long should receive at most the base amount of tokens"
        );

        // Fewer tokens for the same USD size => higher execution price.
        let base_price = prices.index_price_max;
        assert!(
            res.execution_price >= base_price,
            "Execution price for harmful long should be no better (>=) than base price"
        );
    }

    #[test]
    fn short_rounding_uses_min_price_and_rounds_up() {
        let pricing = BasicPricingService::default();
        let impact = BasicPriceImpactService::default();

        let oi = mk_oi(100_000, 100_000, 100_000, 100_000);
        let cfg = ImpactRebalanceConfig::default_quadratic();

        // Choose prices such that size / min_price is fractional,
        // so we can see the ceil behaviour clearly.
        let prices = mk_prices(1_000, 1_050);
        let size_delta_usd: Usd = 10_001;

        let res = pricing
            .get_execution_price_for_increase(
                &impact,
                ExecutionPriceIncreaseParams {
                    oi: &oi,
                    impact_cfg: &cfg,
                    side: Side::Short,
                    size_delta_usd,
                    prices,
                },
            )
            .expect("pricing should succeed");

        // For shorts:
        //   base_tokens = ceil(size_delta_usd / index_price_min)
        let q = size_delta_usd / prices.index_price_min;
        let r = size_delta_usd % prices.index_price_min;
        let expected_base = if r == 0 { q } else { q + 1 };

        assert_eq!(
            res.base_size_delta_tokens, expected_base,
            "Short base tokens must be computed using min price with rounding up (ceil)"
        );
    }
}
