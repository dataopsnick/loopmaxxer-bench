//! GMM Expectation-Maximization Fitter
//!
//! Standard EM algorithm for fitting the 3-component GMM on
//! historical order-flow feature vectors.

use super::model::GmmModel;
use tracing::info;

/// Configuration for the EM algorithm.
#[derive(Debug, Clone)]
pub struct EmConfig {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub regularization: f64,
    pub n_restarts: usize,
}

impl Default for EmConfig {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            tolerance: 1e-6,
            regularization: 1e-6,
            n_restarts: 3,
        }
    }
}

/// EM fitter for the 3-component GMM.
pub struct GmmFitter {
    config: EmConfig,
}

impl GmmFitter {
    pub fn new(config: EmConfig) -> Self {
        Self { config }
    }

    /// Fit the GMM model to the given data using EM.
    pub fn fit(&self, data: &[Vec<f64>], n_features: usize) -> GmmModel {
        if data.is_empty() {
            return GmmModel::new(n_features);
        }

        let mut best_model = GmmModel::new(n_features);
        let mut best_ll = f64::NEG_INFINITY;

        for restart in 0..self.config.n_restarts {
            let mut model = self.initialize_kmeans(data, n_features, restart);
            let ll = self.run_em(&mut model, data);

            if ll > best_ll {
                best_ll = ll;
                best_model = model;
            }
        }

        best_model.log_likelihood = best_ll;
        info!(
            "GMM fit complete: log_likelihood={:.4}, converged={}, iterations={}",
            best_model.log_likelihood, best_model.converged, best_model.n_iterations
        );

        best_model
    }

    fn initialize_kmeans(&self, data: &[Vec<f64>], n_features: usize, seed: usize) -> GmmModel {
        let n = data.len();
        let mut model = GmmModel::new(n_features);

        let idx0 = seed % n;
        let idx1 = (seed + n / 3) % n;
        let idx2 = (seed + 2 * n / 3) % n;

        let indices = [idx0, idx1, idx2];

        for (k, &idx) in indices.iter().enumerate() {
            model.components[k].mean = data[idx].clone();
            let var = 1.0;
            let dim = n_features;
            model.components[k].covariance = vec![0.0; dim * dim];
            model.components[k].cov_inv = vec![0.0; dim * dim];
            for i in 0..dim {
                model.components[k].covariance[i * dim + i] = var;
                model.components[k].cov_inv[i * dim + i] = 1.0 / var;
            }
            model.components[k].cov_det = var.powi(dim as i32);
            model.components[k].weight = 1.0 / 3.0;
        }

        model
    }

    fn run_em(&self, model: &mut GmmModel, data: &[Vec<f64>]) -> f64 {
        let n = data.len();
        let k = 3;
        let dim = model.n_features;

        let mut prev_ll = f64::NEG_INFINITY;

        for iteration in 0..self.config.max_iterations {
            let mut responsibilities = vec![0.0f64; n * k];

            for (i, x) in data.iter().enumerate() {
                let mut weighted_pdfs = [0.0f64; 3];
                let mut total = 0.0;
                for (j, comp) in model.components.iter().enumerate() {
                    let wp = comp.weight * comp.pdf(x);
                    weighted_pdfs[j] = wp;
                    total += wp;
                }

                if total > 1e-300 {
                    for j in 0..k {
                        responsibilities[i * k + j] = weighted_pdfs[j] / total;
                    }
                } else {
                    for j in 0..k {
                        responsibilities[i * k + j] = 1.0 / k as f64;
                    }
                }
            }

            for j in 0..k {
                let nk: f64 = (0..n).map(|i| responsibilities[i * k + j]).sum();

                if nk < 1e-10 {
                    continue;
                }

                model.components[j].weight = nk / n as f64;

                for d in 0..dim {
                    let sum: f64 = (0..n).map(|i| responsibilities[i * k + j] * data[i][d]).sum();
                    model.components[j].mean[d] = sum / nk;
                }

                for r in 0..dim {
                    for c in 0..dim {
                        let sum: f64 = (0..n)
                            .map(|i| {
                                let diff_r = data[i][r] - model.components[j].mean[r];
                                let diff_c = data[i][c] - model.components[j].mean[c];
                                responsibilities[i * k + j] * diff_r * diff_c
                            })
                            .sum();
                        model.components[j].covariance[r * dim + c] =
                            sum / nk + self.config.regularization;
                    }
                }

                model.components[j].update_precomputed();
            }

            let ll = model.log_likelihood(data);
            model.log_likelihood = ll;
            model.n_iterations = iteration + 1;

            if (ll - prev_ll).abs() < self.config.tolerance {
                model.converged = true;
                return ll;
            }

            prev_ll = ll;
        }

        model.converged = false;
        prev_ll
    }
}

impl Default for GmmFitter {
    fn default() -> Self {
        Self::new(EmConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rand_val() -> f64 {
        use std::cell::Cell;
        thread_local! {
            static STATE: Cell<u64> = Cell::new(42);
        }
        STATE.with(|s| {
            let mut v = s.get();
            v = v.wrapping_mul(6364136223846793005).wrapping_add(1);
            s.set(v);
            ((v >> 33) as f64) / (u32::MAX as f64) - 0.5
        })
    }

    #[test]
    fn em_fits_synthetic_data() {
        let mut data = Vec::new();

        for _ in 0..50 {
            data.push(vec![0.0 + rand_val(), 0.0 + rand_val()]);
        }
        for _ in 0..50 {
            data.push(vec![5.0 + rand_val(), 5.0 + rand_val()]);
        }
        for _ in 0..50 {
            data.push(vec![-3.0 + rand_val(), 3.0 + rand_val()]);
        }

        let fitter = GmmFitter::new(EmConfig {
            max_iterations: 50,
            tolerance: 1e-4,
            regularization: 1e-6,
            n_restarts: 2,
        });

        let model = fitter.fit(&data, 2);

        let total_weight: f64 = model.components.iter().map(|c| c.weight).sum();
        assert!(
            (total_weight - 1.0).abs() < 0.1,
            "Weights sum: {}",
            total_weight
        );
        assert!(model.log_likelihood.is_finite());
    }
}