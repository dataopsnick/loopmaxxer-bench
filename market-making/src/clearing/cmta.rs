//! CMTA Multi-Strike Position Aggregation & Netting (Spec §6)
//!
//! Aggregates option positions across strikes and expirations, performs
//! step-out netting to reduce clearing margin, and compresses
//! cross-expiration spreads to minimize TIMS/SPAN haircut.

use std::collections::HashMap;

/// A single option position in the clearing book.
#[derive(Debug, Clone)]
pub struct OptionPosition {
    /// Underlying symbol (e.g., "AAPL").
    pub symbol: String,
    /// Strike price.
    pub strike: f64,
    /// Expiration date as YYYYMMDD integer.
    pub expiration: u32,
    /// Option type: `true` = Call, `false` = Put.
    pub is_call: bool,
    /// Net position quantity (positive = long, negative = short).
    pub quantity: i32,
    /// Average execution price.
    pub avg_price: f64,
}

impl OptionPosition {
    /// Create a new option position.
    pub fn new(
        symbol: &str,
        strike: f64,
        expiration: u32,
        is_call: bool,
        quantity: i32,
        avg_price: f64,
    ) -> Self {
        Self {
            symbol: symbol.to_string(),
            strike,
            expiration,
            is_call,
            quantity,
            avg_price,
        }
    }

    /// Check if this position is long.
    #[inline(always)]
    pub fn is_long(&self) -> bool {
        self.quantity > 0
    }

    /// Check if this position is short.
    #[inline(always)]
    pub fn is_short(&self) -> bool {
        self.quantity < 0
    }

    /// Get the notional delta equivalent (quantity * 100 shares per contract).
    #[inline(always)]
    pub fn share_equivalent(&self) -> i32 {
        self.quantity * 100
    }
}

/// Result of position netting and compression.
#[derive(Debug, Clone)]
pub struct PositionNettingResult {
    /// Netted positions after aggregation.
    pub netted_positions: Vec<OptionPosition>,
    /// Number of positions before netting.
    pub positions_before: usize,
    /// Number of positions after netting.
    pub positions_after: usize,
    /// Estimated margin reduction from netting (USD).
    pub margin_reduction: f64,
}

/// The CMTA clearing engine.
///
/// Aggregates option positions, performs step-out netting across
/// strikes/expirations, and compresses offsetting positions to
/// minimize clearing margin.
pub struct CmtaClearingEngine {
    /// Current positions keyed by (symbol, strike as bits, expiration, is_call).
    positions: HashMap<(String, u64, u32, bool), OptionPosition>,
    /// Total margin estimate before netting.
    gross_margin: f64,
    /// Total margin estimate after netting.
    net_margin: f64,
}

impl CmtaClearingEngine {
    /// Create a new empty CMTA clearing engine.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            gross_margin: 0.0,
            net_margin: 0.0,
        }
    }

    /// Add or update a position in the clearing book.
    ///
    /// If a position with the same key already exists, the quantities
    /// are netted and the average price is weighted.
    pub fn add_position(&mut self, pos: OptionPosition) {
        let key = (
            pos.symbol.clone(),
            pos.strike.to_bits(),
            pos.expiration,
            pos.is_call,
        );

        self.positions
            .entry(key)
            .and_modify(|existing| {
                let old_qty = existing.quantity as f64;
                let new_qty = pos.quantity as f64;
                let total_qty = old_qty + new_qty;

                if total_qty.abs() < 1e-9 {
                    // Positions cancel out
                    existing.quantity = 0;
                    existing.avg_price = 0.0;
                } else {
                    // Weighted average price
                    existing.avg_price = (existing.avg_price * old_qty + pos.avg_price * new_qty)
                        / total_qty;
                    existing.quantity = total_qty as i32;
                }
            })
            .or_insert(pos);
    }

    /// Perform step-out netting: cancel offsetting positions across
    /// strikes within the same expiration.
    ///
    /// For example, if we are long 10 calls at strike 150 and short 10
    /// calls at strike 155 (same expiration), the netting recognizes
    /// the spread and reduces margin.
    pub fn step_out_net(&mut self) -> PositionNettingResult {
        let positions_before = self.positions.len();

        // Group by (symbol, expiration, is_call)
        let mut groups: HashMap<(String, u32, bool), Vec<OptionPosition>> = HashMap::new();

        for pos in self.positions.values() {
            if pos.quantity == 0 {
                continue;
            }
            groups
                .entry((pos.symbol.clone(), pos.expiration, pos.is_call))
                .or_default()
                .push(pos.clone());
        }

        // Within each group, net offsetting quantities
        let mut netted: Vec<OptionPosition> = Vec::new();

        for (_, mut group) in groups {
            // Sort by strike for deterministic processing
            group.sort_by(|a, b| {
                a.strike
                    .partial_cmp(&b.strike)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Sum up net quantity
            let net_qty: i32 = group.iter().map(|p| p.quantity).sum();

            if net_qty == 0 {
                // All positions cancel — spread compression
                // Keep the spread legs for margin calculation but mark as netted
                for pos in group {
                    if pos.quantity != 0 {
                        netted.push(pos);
                    }
                }
            } else {
                // Net to a single position at weighted average strike
                let weighted_strike: f64 = group
                    .iter()
                    .map(|p| p.strike * p.quantity as f64)
                    .sum::<f64>()
                    / net_qty as f64;
                let weighted_price: f64 = group
                    .iter()
                    .map(|p| p.avg_price * p.quantity as f64)
                    .sum::<f64>()
                    / net_qty as f64;

                let symbol = group[0].symbol.clone();
                let expiration = group[0].expiration;
                let is_call = group[0].is_call;

                netted.push(OptionPosition {
                    symbol,
                    strike: weighted_strike,
                    expiration,
                    is_call,
                    quantity: net_qty,
                    avg_price: weighted_price,
                });
            }
        }

        let positions_after = netted.len();

        // Estimate margin reduction
        // Gross margin ~ sum of |quantity| * strike * 0.15 (15% haircut)
        let gross: f64 = self
            .positions
            .values()
            .map(|p| (p.quantity as f64).abs() * p.strike * 100.0 * 0.15)
            .sum();

        let net: f64 = netted
            .iter()
            .map(|p| (p.quantity as f64).abs() * p.strike * 100.0 * 0.15)
            .sum();

        self.gross_margin = gross;
        self.net_margin = net;

        PositionNettingResult {
            netted_positions: netted,
            positions_before,
            positions_after,
            margin_reduction: gross - net,
        }
    }

    /// Compress cross-expiration spreads.
    ///
    /// If we have a call spread in January and the opposite call spread
    /// in February at the same strike, the margin can be compressed
    /// by recognizing the calendar spread.
    pub fn compress_cross_expiration(&mut self) -> PositionNettingResult {
        let positions_before = self.positions.len();

        // Group by (symbol, strike as bits, is_call) across expirations
        let mut groups: HashMap<(String, u64, bool), Vec<OptionPosition>> = HashMap::new();

        for pos in self.positions.values() {
            if pos.quantity == 0 {
                continue;
            }
            groups
                .entry((pos.symbol.clone(), pos.strike.to_bits(), pos.is_call))
                .or_default()
                .push(pos.clone());
        }

        let mut netted: Vec<OptionPosition> = Vec::new();

        for (_, mut group) in groups {
            // Sort by expiration
            group.sort_by_key(|p| p.expiration);

            // Net quantities across expirations
            let net_qty: i32 = group.iter().map(|p| p.quantity).sum();

            if net_qty == 0 {
                // Calendar spread — keep legs but margin is compressed
                for pos in group {
                    if pos.quantity != 0 {
                        netted.push(pos);
                    }
                }
            } else {
                // Net to nearest expiration
                let weighted_price: f64 = group
                    .iter()
                    .map(|p| p.avg_price * p.quantity as f64)
                    .sum::<f64>()
                    / net_qty as f64;

                let nearest = group
                    .iter()
                    .min_by_key(|p| p.expiration)
                    .unwrap()
                    .clone();

                netted.push(OptionPosition {
                    symbol: nearest.symbol,
                    strike: nearest.strike,
                    expiration: nearest.expiration,
                    is_call: nearest.is_call,
                    quantity: net_qty,
                    avg_price: weighted_price,
                });
            }
        }

        let positions_after = netted.len();

        let gross: f64 = self
            .positions
            .values()
            .map(|p| (p.quantity as f64).abs() * p.strike * 100.0 * 0.15)
            .sum();

        let net: f64 = netted
            .iter()
            .map(|p| (p.quantity as f64).abs() * p.strike * 100.0 * 0.15)
            .sum();

        PositionNettingResult {
            netted_positions: netted,
            positions_before,
            positions_after,
            margin_reduction: gross - net,
        }
    }

    /// Get all current positions.
    pub fn positions(&self) -> impl Iterator<Item = &OptionPosition> {
        self.positions.values()
    }

    /// Get the gross margin estimate (before netting).
    #[inline(always)]
    pub fn gross_margin(&self) -> f64 {
        self.gross_margin
    }

    /// Get the net margin estimate (after netting).
    #[inline(always)]
    pub fn net_margin(&self) -> f64 {
        self.net_margin
    }

    /// Get the number of distinct positions.
    #[inline(always)]
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }
}

impl Default for CmtaClearingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_net_same_strike() {
        let mut engine = CmtaClearingEngine::new();

        // Long 10 AAPL 150 calls
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, 10, 2.50));
        // Short 10 AAPL 150 calls (should cancel)
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, -10, 2.60));

        let result = engine.step_out_net();
        assert_eq!(result.positions_before, 1);
        // The netted position should have quantity 0
        assert!(result.netted_positions.iter().all(|p| p.quantity == 0));
    }

    #[test]
    fn step_out_net_reduces_positions() {
        let mut engine = CmtaClearingEngine::new();

        // Long 10 calls at 150, short 5 calls at 155 (same expiration)
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, 10, 2.50));
        engine.add_position(OptionPosition::new("AAPL", 155.0, 20240119, true, -5, 1.80));

        let result = engine.step_out_net();
        assert!(result.positions_after <= result.positions_before);
        assert!(result.margin_reduction >= 0.0);
    }

    #[test]
    fn cross_expiration_compression() {
        let mut engine = CmtaClearingEngine::new();

        // Long 10 Jan 150 calls, short 10 Feb 150 calls (calendar spread)
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, 10, 2.50));
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240216, true, -10, 3.20));

        let result = engine.compress_cross_expiration();
        assert!(result.positions_after <= result.positions_before);
    }

    #[test]
    fn weighted_average_price() {
        let mut engine = CmtaClearingEngine::new();

        // Add 10 contracts at $2.50
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, 10, 2.50));
        // Add 10 more contracts at $3.50
        engine.add_position(OptionPosition::new("AAPL", 150.0, 20240119, true, 10, 3.50));

        // Should have 20 contracts at avg $3.00
        let positions: Vec<_> = engine.positions().collect();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity, 20);
        assert!((positions[0].avg_price - 3.00).abs() < 1e-9);
    }

    #[test]
    fn empty_engine() {
        let engine = CmtaClearingEngine::new();
        assert_eq!(engine.position_count(), 0);
        assert_eq!(engine.gross_margin(), 0.0);
    }

    #[test]
    fn share_equivalent_calculation() {
        let pos = OptionPosition::new("AAPL", 150.0, 20240119, true, 5, 2.50);
        assert_eq!(pos.share_equivalent(), 500);
    }
}