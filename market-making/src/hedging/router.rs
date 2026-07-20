//! Hedging Routing Matrix implementation (Spec §22, §25)
//!
//! Solves the quadratic program:
//!   h* = argmin { γ·hᵀΣh + hᵀc_spread + hᵀr_carry }
//!   subject to: wᵀh + ΔD = 0
//!
//! For the 2-asset case (stock + future), uses a closed-form analytic
//! solution. For N assets, uses projected gradient descent.

/// Hedge allocation result.
#[derive(Debug, Clone, Copy)]
pub struct HedgeAllocation {
    /// Quantity allocated to each hedge instrument.
    pub quantities: [f64; 3],
    /// Total cost of the hedge (spread + carry).
    pub total_cost: f64,
    /// Residual delta after hedging (should be ~0).
    pub residual_delta: f64,
}

impl Default for HedgeAllocation {
    fn default() -> Self {
        Self {
            quantities: [0.0; 3],
            total_cost: 0.0,
            residual_delta: 0.0,
        }
    }
}

impl HedgeAllocation {
    /// Stock allocation quantity.
    #[inline(always)]
    pub fn stock_qty(&self) -> f64 {
        self.quantities[0]
    }

    /// Future allocation quantity.
    #[inline(always)]
    pub fn future_qty(&self) -> f64 {
        self.quantities[1]
    }

    /// Basket allocation quantity.
    #[inline(always)]
    pub fn basket_qty(&self) -> f64 {
        self.quantities[2]
    }
}

/// Smart hedging routing matrix (Spec §22, §25).
///
/// Determines the optimal split of a delta imbalance across stock,
/// future, and basket hedge instruments to minimize total financial
/// friction cost (variance + spread + carry).
pub struct HedgingRoutingMatrix {
    /// Stock slippage coefficient (market impact).
    stock_slippage_coef: f64,
    /// Future slippage coefficient.
    future_slippage_coef: f64,
    /// Basket slippage coefficient.
    basket_slippage_coef: f64,
    /// Risk aversion coefficient γ.
    risk_aversion: f64,
}

impl HedgingRoutingMatrix {
    /// Create a new routing matrix with the given slippage coefficients.
    pub const fn new(stock_slippage: f64, future_slippage: f64) -> Self {
        Self {
            stock_slippage_coef: stock_slippage,
            future_slippage_coef: future_slippage,
            basket_slippage_coef: stock_slippage * 1.5,
            risk_aversion: 0.015,
        }
    }

    /// Create with full parameters including basket and risk aversion.
    pub const fn with_params(
        stock_slippage: f64,
        future_slippage: f64,
        basket_slippage: f64,
        risk_aversion: f64,
    ) -> Self {
        Self {
            stock_slippage_coef: stock_slippage,
            future_slippage_coef: future_slippage,
            basket_slippage_coef: basket_slippage,
            risk_aversion,
        }
    }

    /// Determine optimal hedging allocation for a delta imbalance using
    /// the closed-form 2-asset solution (stock + future).
    ///
    /// Cost functions:
    ///   C_stock = stock_slippage · x² + x · sofr_borrow
    ///   C_future = future_slippage · y² + y · span_opportunity
    ///   Constraint: x + y = imbalance
    #[inline(always)]
    pub fn determine_optimal_hedging_allocation(
        &self,
        imbalance: f64,
        sofr_borrow_rate: f64,
        future_span_opportunity_cost: f64,
    ) -> (f64, f64) {
        if imbalance.abs() < 1e-5 {
            return (0.0, 0.0);
        }

        // Closed-form solution from partial derivative equations:
        // ∂C/∂x = 2·stock_slippage·x + sofr_borrow - λ = 0
        // ∂C/∂y = 2·future_slippage·y + span_opportunity - λ = 0
        // x + y = imbalance
        let num = 2.0 * self.future_slippage_coef * imbalance
            + future_span_opportunity_cost
            - sofr_borrow_rate;
        let den = 2.0 * (self.stock_slippage_coef + self.future_slippage_coef);

        let stock_alloc = (num / den).clamp(0.0, imbalance.abs()) * imbalance.signum();
        let future_alloc = imbalance - stock_alloc;

        (stock_alloc, future_alloc)
    }

    /// Full 3-asset allocation using projected gradient descent.
    ///
    /// Solves: min { γ·hᵀΣh + hᵀc_spread + hᵀr_carry }
    /// subject to: wᵀh + ΔD = 0
    pub fn determine_3asset_allocation(
        &self,
        imbalance: f64,
        sofr_borrow_rate: f64,
        future_span_cost: f64,
        basket_borrow_rate: f64,
    ) -> HedgeAllocation {
        if imbalance.abs() < 1e-5 {
            return HedgeAllocation::default();
        }

        // Delta conversion weights: stock=1.0, future=multiplier, basket=beta
        let weights = [1.0, 0.95, 0.80];

        // Carry cost vector
        let carry = [sofr_borrow_rate, future_span_cost, basket_borrow_rate];

        // Slippage coefficients (diagonal of Σ)
        let slippage = [
            self.stock_slippage_coef,
            self.future_slippage_coef,
            self.basket_slippage_coef,
        ];

        // Projected gradient descent
        let mut h = [0.0f64; 3];
        let lr = 0.01;
        let n_iters = 200;

        // Initialize: proportional allocation
        let total_weight: f64 = weights.iter().sum();
        for i in 0..3 {
            h[i] = -imbalance * weights[i] / total_weight;
        }

        for _ in 0..n_iters {
            // Gradient: 2·γ·Σh + c_spread + r_carry
            let mut grad = [0.0f64; 3];
            for i in 0..3 {
                grad[i] = 2.0 * self.risk_aversion * slippage[i] * h[i] + slippage[i] + carry[i];
            }

            // Project gradient onto constraint surface: wᵀgrad = 0
            let dot: f64 = grad.iter().zip(weights.iter()).map(|(g, w)| g * w).sum();
            let w_sq: f64 = weights.iter().map(|w| w * w).sum();
            for i in 0..3 {
                grad[i] -= dot * weights[i] / w_sq;
            }

            // Update
            for i in 0..3 {
                h[i] -= lr * grad[i];
            }

            // Re-project to satisfy constraint exactly
            let current_delta: f64 = h.iter().zip(weights.iter()).map(|(hi, w)| hi * w).sum();
            let correction = (current_delta + imbalance) / w_sq;
            for i in 0..3 {
                h[i] -= correction * weights[i];
            }
        }

        // Compute total cost
        let mut total_cost = 0.0;
        for i in 0..3 {
            total_cost += self.risk_aversion * slippage[i] * h[i] * h[i]
                + slippage[i] * h[i].abs()
                + carry[i] * h[i].abs();
        }

        let residual: f64 = h.iter().zip(weights.iter()).map(|(hi, w)| hi * w).sum::<f64>() + imbalance;

        HedgeAllocation {
            quantities: h,
            total_cost,
            residual_delta: residual,
        }
    }
}

impl Default for HedgingRoutingMatrix {
    fn default() -> Self {
        Self::new(0.001, 0.0005)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_imbalance_returns_zero() {
        let router = HedgingRoutingMatrix::default();
        let (stock, future) = router.determine_optimal_hedging_allocation(0.0, 0.05, 0.03);
        assert!((stock - 0.0).abs() < 1e-9);
        assert!((future - 0.0).abs() < 1e-9);
    }

    #[test]
    fn allocation_sums_to_imbalance() {
        let router = HedgingRoutingMatrix::default();
        let imbalance = 1000.0;
        let (stock, future) =
            router.determine_optimal_hedging_allocation(imbalance, 0.05, 0.03);
        let total = stock + future;
        assert!(
            (total - imbalance).abs() < 1e-6,
            "Stock + Future should equal imbalance: {} + {} = {} vs {}",
            stock,
            future,
            total,
            imbalance
        );
    }

    #[test]
    fn cheaper_future_gets_more_allocation() {
        let router = HedgingRoutingMatrix::default();
        let imbalance = 1000.0;

        // Low future cost → more future allocation
        let (stock_low, future_low) =
            router.determine_optimal_hedging_allocation(imbalance, 0.05, 0.01);

        // High future cost → more stock allocation
        let (stock_high, future_high) =
            router.determine_optimal_hedging_allocation(imbalance, 0.05, 0.10);

        assert!(
            future_low > future_high,
            "Lower future cost should increase future allocation: {} > {}",
            future_low,
            future_high
        );
        assert!(
            stock_low < stock_high,
            "Higher future cost should increase stock allocation: {} < {}",
            stock_low,
            stock_high
        );
    }

    #[test]
    fn three_asset_allocation_satisfies_constraint() {
        let router = HedgingRoutingMatrix::default();
        let result = router.determine_3asset_allocation(1000.0, 0.05, 0.03, 0.04);

        // Constraint: wᵀh + ΔD ≈ 0
        assert!(
            result.residual_delta.abs() < 1.0,
            "Residual delta should be near zero: {}",
            result.residual_delta
        );
    }

    #[test]
    fn three_asset_zero_imbalance() {
        let router = HedgingRoutingMatrix::default();
        let result = router.determine_3asset_allocation(0.0, 0.05, 0.03, 0.04);
        assert!((result.stock_qty() - 0.0).abs() < 1e-9);
        assert!((result.future_qty() - 0.0).abs() < 1e-9);
    }
}