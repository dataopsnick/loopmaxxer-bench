//! VWAP Slippage & Price-Impact Metrics
//!
//! Computes the real historical Volume-Weighted Average Price (VWAP) from
//! `MarketEvent::Trade` events and measures the bookmaker's execution
//! slippage against it, plus a simple linear ("Kyle's lambda") price-impact
//! regression of price change on signed order flow — used to compare the
//! bookmaker's simulated market impact against the real historical market
//! impact on each CPCV out-of-sample fold.

use crate::iex::MarketEvent;
use crate::memorydb::vector_store::FeatureVector;
use crate::simulation::FillRecord;
use serde::Serialize;

/// Compute the volume-weighted average price over a slice of market
/// events, using only `Trade` events: `VWAP = Σ(price·size) / Σ(size)`.
pub fn compute_vwap(events: &[MarketEvent]) -> f64 {
    let mut notional = 0.0f64;
    let mut volume = 0.0f64;

    for event in events {
        if let MarketEvent::Trade { price, size, .. } = event {
            notional += price * size;
            volume += size;
        }
    }

    if volume > 1e-12 {
        notional / volume
    } else {
        0.0
    }
}

/// Aggregated VWAP slippage statistics for a set of bookmaker fills
/// against a reference (real historical) VWAP.
#[derive(Debug, Clone, Serialize)]
pub struct VwapSlippageStats {
    /// Notional (size-weighted) mean slippage in basis points, signed so
    /// that positive means the bookmaker executed *worse* than the real
    /// market VWAP.
    pub notional_weighted_slippage_bps: f64,
    /// Simple (unweighted) mean per-fill slippage in basis points.
    pub mean_slippage_bps: f64,
    /// Standard deviation of per-fill slippage in basis points.
    pub std_slippage_bps: f64,
    /// Number of fills used in the computation.
    pub n_fills: usize,
}

/// Compute VWAP slippage statistics for `fills` against `market_vwap`.
///
/// Per-fill signed slippage (in bps) is defined so that positive values
/// always mean *worse* execution than the real market VWAP: a BUY fill
/// priced above VWAP, or a SELL fill priced below VWAP.
pub fn vwap_slippage(fills: &[FillRecord], market_vwap: f64) -> VwapSlippageStats {
    if fills.is_empty() || market_vwap.abs() < 1e-12 {
        return VwapSlippageStats {
            notional_weighted_slippage_bps: 0.0,
            mean_slippage_bps: 0.0,
            std_slippage_bps: 0.0,
            n_fills: 0,
        };
    }

    let mut per_fill_bps = Vec::with_capacity(fills.len());
    let mut notional_sum = 0.0f64;
    let mut weighted_sum = 0.0f64;

    for fill in fills {
        let sign = if fill.side == "BUY" { 1.0 } else { -1.0 };
        let bps = sign * (fill.price - market_vwap) / market_vwap * 10_000.0;
        per_fill_bps.push(bps);

        let notional = fill.price * fill.size;
        notional_sum += notional;
        weighted_sum += notional * bps;
    }

    let n = per_fill_bps.len() as f64;
    let mean = per_fill_bps.iter().sum::<f64>() / n;
    let variance = per_fill_bps.iter().map(|b| (b - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();

    let notional_weighted = if notional_sum > 1e-12 {
        weighted_sum / notional_sum
    } else {
        mean
    };

    VwapSlippageStats {
        notional_weighted_slippage_bps: notional_weighted,
        mean_slippage_bps: mean,
        std_slippage_bps: std,
        n_fills: fills.len(),
    }
}

/// Result of a simple linear ("Kyle's lambda") price-impact regression:
/// `Δp = λ · signed_flow + ε`.
#[derive(Debug, Clone, Serialize)]
pub struct PriceImpactEstimate {
    /// Estimated price-impact coefficient λ (price change per unit of
    /// signed order flow).
    pub lambda: f64,
    /// Coefficient of determination (R²) of the fit, in `[0, 1]`.
    pub r_squared: f64,
    /// Number of (Δp, signed_flow) observation pairs used.
    pub n_obs: usize,
}

/// Ordinary-least-squares single-regressor fit of `y` on `x` through the
/// origin (`y = λx`), matching the standard Kyle's-lambda specification
/// which regresses price changes on signed order flow without an
/// intercept.
fn ols_through_origin(x: &[f64], y: &[f64]) -> PriceImpactEstimate {
    let n = x.len().min(y.len());
    if n < 2 {
        return PriceImpactEstimate {
            lambda: 0.0,
            r_squared: 0.0,
            n_obs: n,
        };
    }

    let sum_xy: f64 = (0..n).map(|i| x[i] * y[i]).sum();
    let sum_xx: f64 = (0..n).map(|i| x[i] * x[i]).sum();

    if sum_xx < 1e-12 {
        return PriceImpactEstimate {
            lambda: 0.0,
            r_squared: 0.0,
            n_obs: n,
        };
    }

    let lambda = sum_xy / sum_xx;

    let ss_tot: f64 = (0..n).map(|i| y[i] * y[i]).sum();
    let ss_res: f64 = (0..n)
        .map(|i| {
            let resid = y[i] - lambda * x[i];
            resid * resid
        })
        .sum();

    let r_squared = if ss_tot > 1e-12 {
        (1.0 - ss_res / ss_tot).max(0.0)
    } else {
        0.0
    };

    PriceImpactEstimate {
        lambda,
        r_squared,
        n_obs: n,
    }
}

/// Estimate the real historical price impact λ by regressing consecutive
/// trade-to-trade price changes on signed trade flow across `events`.
///
/// Uses `Trade` events only: the signed flow of a trade is inferred
/// relative to the previous trade price (an uptick is a buy, a downtick
/// is a sell — the standard tick rule), and `Δp` is the trade-to-trade
/// price change.
pub fn estimate_price_impact(events: &[MarketEvent]) -> PriceImpactEstimate {
    let mut signed_flows = Vec::new();
    let mut delta_prices = Vec::new();

    let mut prev_price: Option<f64> = None;
    for event in events {
        if let MarketEvent::Trade { price, size, .. } = event {
            if let Some(pp) = prev_price {
                let delta_p = price - pp;
                let signed_flow = if *price >= pp { *size } else { -*size };
                delta_prices.push(delta_p);
                signed_flows.push(signed_flow);
            }
            prev_price = Some(*price);
        }
    }

    ols_through_origin(&signed_flows, &delta_prices)
}

/// Estimate the bookmaker's *simulated* price impact by regressing
/// consecutive fill-to-fill price changes on the fills' signed size.
///
/// This mirrors `estimate_price_impact` but on the bookmaker's simulated
/// `FillRecord`s, allowing a direct real-vs-simulated Δλ comparison on
/// the same out-of-sample fold. `features` is accepted for API symmetry
/// with the rest of the CPCV evaluator pipeline (future extension:
/// weighting by realized volatility) but is currently unused beyond an
/// emptiness guard.
pub fn bookmaker_implied_impact(
    fills: &[FillRecord],
    features: &[FeatureVector],
) -> PriceImpactEstimate {
    if fills.len() < 2 || features.is_empty() {
        return PriceImpactEstimate {
            lambda: 0.0,
            r_squared: 0.0,
            n_obs: 0,
        };
    }

    let mut signed_flows = Vec::with_capacity(fills.len() - 1);
    let mut delta_prices = Vec::with_capacity(fills.len() - 1);

    for i in 1..fills.len() {
        let delta_p = fills[i].price - fills[i - 1].price;
        let sign = if fills[i].side == "BUY" { 1.0 } else { -1.0 };
        signed_flows.push(sign * fills[i].size);
        delta_prices.push(delta_p);
    }

    ols_through_origin(&signed_flows, &delta_prices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(price: f64, size: f64, ts: u64) -> MarketEvent {
        MarketEvent::Trade {
            symbol: "AAPL".to_string(),
            price,
            size,
            timestamp_ns: ts,
        }
    }

    fn fill(side: &str, price: f64, size: f64, ts: u64) -> FillRecord {
        FillRecord {
            timestamp_ns: ts,
            side: side.to_string(),
            price,
            size,
            pnl: 0.0,
        }
    }

    #[test]
    fn vwap_matches_known_average() {
        let events = vec![
            trade(100.0, 10.0, 1),
            trade(102.0, 30.0, 2),
            trade(98.0, 10.0, 3),
        ];
        let vwap = compute_vwap(&events);
        // (100*10 + 102*30 + 98*10) / 50 = (1000+3060+980)/50 = 100.8
        assert!((vwap - 100.8).abs() < 1e-9, "vwap={}", vwap);
    }

    #[test]
    fn vwap_zero_volume_returns_zero() {
        let events = vec![MarketEvent::QuoteUpdate {
            symbol: "AAPL".to_string(),
            bid_price: 100.0,
            bid_size: 10.0,
            ask_price: 100.1,
            ask_size: 10.0,
            timestamp_ns: 1,
        }];
        assert_eq!(compute_vwap(&events), 0.0);
    }

    #[test]
    fn slippage_positive_when_buy_above_vwap() {
        let fills = vec![fill("BUY", 101.0, 100.0, 1)];
        let stats = vwap_slippage(&fills, 100.0);
        assert!(
            stats.mean_slippage_bps > 0.0,
            "buying above VWAP should register positive (worse) slippage: {:?}",
            stats
        );
    }

    #[test]
    fn slippage_positive_when_sell_below_vwap() {
        let fills = vec![fill("SELL", 99.0, 100.0, 1)];
        let stats = vwap_slippage(&fills, 100.0);
        assert!(
            stats.mean_slippage_bps > 0.0,
            "selling below VWAP should register positive (worse) slippage: {:?}",
            stats
        );
    }

    #[test]
    fn slippage_empty_fills_is_zero() {
        let stats = vwap_slippage(&[], 100.0);
        assert_eq!(stats.n_fills, 0);
        assert_eq!(stats.mean_slippage_bps, 0.0);
    }

    #[test]
    fn price_impact_recovers_known_lambda() {
        let lambda_true = 0.01;
        let mut price = 100.0;
        let mut events = vec![trade(price, 100.0, 0)];
        for i in 1..50 {
            let flow: f64 = if i % 2 == 0 { 50.0 } else { -30.0 };
            price += lambda_true * flow;
            events.push(trade(price, flow.abs(), i as u64));
        }

        let estimate = estimate_price_impact(&events);
        assert!(
            (estimate.lambda - lambda_true).abs() < 1e-6,
            "lambda={} expected~{}",
            estimate.lambda,
            lambda_true
        );
        assert!(estimate.r_squared > 0.99, "r_squared={}", estimate.r_squared);
    }

    #[test]
    fn price_impact_insufficient_data_is_zero() {
        let events = vec![trade(100.0, 10.0, 0)];
        let estimate = estimate_price_impact(&events);
        assert_eq!(estimate.lambda, 0.0);
        assert_eq!(estimate.n_obs, 0);
    }

    #[test]
    fn bookmaker_impact_recovers_known_lambda() {
        let lambda_true = 0.02;
        let mut price = 150.0;
        let mut fills = vec![fill("BUY", price, 10.0, 0)];
        for i in 1..20 {
            let side = if i % 2 == 0 { "BUY" } else { "SELL" };
            let size = 20.0;
            let signed = if side == "BUY" { size } else { -size };
            price += lambda_true * signed;
            fills.push(fill(side, price, size, i as u64));
        }

        let features = vec![FeatureVector {
            timestamp_ns: 0,
            symbol: "AAPL".to_string(),
            normalized_trade_size: 0.01,
            signed_order_flow: 10.0,
            ofi_ewma: 0.0,
            spread_width: 0.02,
            vol_atm: 0.2,
            return_predictability: 0.0,
        }];

        let estimate = bookmaker_implied_impact(&fills, &features);
        assert!(
            (estimate.lambda - lambda_true).abs() < 1e-6,
            "lambda={} expected~{}",
            estimate.lambda,
            lambda_true
        );
    }

    #[test]
    fn bookmaker_impact_empty_is_zero() {
        let estimate = bookmaker_implied_impact(&[], &[]);
        assert_eq!(estimate.lambda, 0.0);
        assert_eq!(estimate.n_obs, 0);
    }
}

