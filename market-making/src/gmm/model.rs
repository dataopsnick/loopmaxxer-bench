//! GMM Model Definition
//!
//! 3-component Gaussian Mixture Model for trader type classification.

use serde::{Deserialize, Serialize};

/// The three hidden trader states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraderState {
    /// Noise trader: symmetric, no information
    Noise = 0,
    /// Institutional buyer/seller: persistent directional flow
    Institutional = 1,
    /// Informed insider: adverse selection, correlated with future returns
    Informed = 2,
}

impl TraderState {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Noise,
            1 => Self::Institutional,
            2 => Self::Informed,
            _ => Self::Noise,
        }
    }
}

/// A single Gaussian component in the mixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmmComponent {
    /// Mixing weight π_k
    pub weight: f64,
    /// Mean vector μ_k (length = n_features)
    pub mean: Vec<f64>,
    /// Covariance matrix Σ_k (stored as flat n_features x n_features)
    pub covariance: Vec<f64>,
    /// Precomputed determinant of covariance (for likelihood evaluation)
    pub cov_det: f64,
    /// Precomputed inverse covariance (for Mahalanobis distance)
    pub cov_inv: Vec<f64>,
    /// Dimensionality
    pub dim: usize,
}

impl GmmComponent {
    pub fn new(dim: usize) -> Self {
        Self {
            weight: 1.0 / 3.0,
            mean: vec![0.0; dim],
            covariance: vec![0.0; dim * dim],
            cov_det: 1.0,
            cov_inv: vec![0.0; dim * dim],
            dim,
        }
    }

    /// Initialize as isotropic Gaussian with given mean and variance.
    pub fn isotropic(mean: Vec<f64>, variance: f64, weight: f64) -> Self {
        let dim = mean.len();
        let mut cov = vec![0.0; dim * dim];
        let mut cov_inv = vec![0.0; dim * dim];
        for i in 0..dim {
            cov[i * dim + i] = variance;
            cov_inv[i * dim + i] = 1.0 / variance;
        }
        let det = variance.powi(dim as i32);
        Self {
            weight,
            mean,
            covariance: cov,
            cov_det: det,
            cov_inv,
            dim,
        }
    }

    /// Evaluate the multivariate Gaussian PDF at point x.
    ///
    /// p(x) = (2π)^(-d/2) * |Σ|^(-1/2) * exp(-0.5 * (x-μ)^T Σ^-1 (x-μ))
    #[inline(always)]
    pub fn pdf(&self, x: &[f64]) -> f64 {
        if x.len() != self.dim {
            return 0.0;
        }

        // Mahalanobis distance: (x-μ)^T Σ^-1 (x-μ)
        let mut mahal = 0.0;
        for i in 0..self.dim {
            let diff_i = x[i] - self.mean[i];
            for j in 0..self.dim {
                let diff_j = x[j] - self.mean[j];
                mahal += diff_i * self.cov_inv[i * self.dim + j] * diff_j;
            }
        }

        let det_term = if self.cov_det > 1e-300 {
            self.cov_det.sqrt().recip()
        } else {
            1e300
        };

        let two_pi = 2.0 * std::f64::consts::PI;
        let norm_const = two_pi.powf(-(self.dim as f64) / 2.0);

        norm_const * det_term * (-0.5 * mahal).exp()
    }

    /// Update the precomputed determinant and inverse from the covariance matrix.
    pub fn update_precomputed(&mut self) {
        self.cov_det = matrix_determinant(&self.covariance, self.dim);
        if let Some(inv) = matrix_inverse(&self.covariance, self.dim) {
            self.cov_inv = inv;
        }
    }
}

/// 3-component GMM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmmModel {
    pub components: [GmmComponent; 3],
    pub n_features: usize,
    pub log_likelihood: f64,
    pub n_iterations: usize,
    pub converged: bool,
}

impl GmmModel {
    /// Create a new 3-component GMM with the given feature dimensionality.
    pub fn new(n_features: usize) -> Self {
        let components = [
            GmmComponent::isotropic(vec![0.0; n_features], 1.0, 1.0 / 3.0),
            GmmComponent::isotropic(vec![0.5; n_features], 2.0, 1.0 / 3.0),
            GmmComponent::isotropic(vec![-0.5; n_features], 0.5, 1.0 / 3.0),
        ];

        Self {
            components,
            n_features,
            log_likelihood: f64::NEG_INFINITY,
            n_iterations: 0,
            converged: false,
        }
    }

    /// Compute the posterior probability P(state=k | x) for each component.
    ///
    /// Returns [p_noise, p_institutional, p_informed]
    pub fn posterior(&self, x: &[f64]) -> [f64; 3] {
        let mut weighted_pdfs = [0.0f64; 3];
        let mut total = 0.0;

        for (k, comp) in self.components.iter().enumerate() {
            let wp = comp.weight * comp.pdf(x);
            weighted_pdfs[k] = wp;
            total += wp;
        }

        if total > 1e-300 {
            [
                weighted_pdfs[0] / total,
                weighted_pdfs[1] / total,
                weighted_pdfs[2] / total,
            ]
        } else {
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]
        }
    }

    /// Classify a single observation into the most likely trader state.
    pub fn classify(&self, x: &[f64]) -> TraderState {
        let post = self.posterior(x);
        let max_idx = post
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        TraderState::from_index(max_idx)
    }

    /// Compute the total log-likelihood of the data.
    pub fn log_likelihood(&self, data: &[Vec<f64>]) -> f64 {
        let mut ll = 0.0;
        for x in data {
            let mut p = 0.0;
            for comp in &self.components {
                p += comp.weight * comp.pdf(x);
            }
            if p > 0.0 {
                ll += p.ln();
            } else {
                ll += -1e10;
            }
        }
        ll
    }

    /// Get the mixture weight for the noise trader component.
    pub fn pi_noise(&self) -> f64 {
        self.components[0].weight
    }

    /// Get the mixture weight for the institutional component.
    pub fn pi_institutional(&self) -> f64 {
        self.components[1].weight
    }

    /// Get the mixture weight for the informed insider component.
    pub fn pi_informed(&self) -> f64 {
        self.components[2].weight
    }

    /// Get the mean signed order flow for the informed component.
    pub fn informed_mean_flow(&self) -> f64 {
        self.components[2].mean.get(1).copied().unwrap_or(0.0)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Serialization error: {}", e))
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Deserialization error: {}", e))
    }
}

/// Compute the determinant of a square matrix (Gaussian elimination).
fn matrix_determinant(mat: &[f64], dim: usize) -> f64 {
    if dim == 0 {
        return 1.0;
    }
    if dim == 1 {
        return mat[0];
    }
    if dim == 2 {
        return mat[0] * mat[3] - mat[1] * mat[2];
    }

    let mut a = mat.to_vec();
    let mut det = 1.0;

    for i in 0..dim {
        let mut max_val = a[i * dim + i].abs();
        let mut max_row = i;
        for k in (i + 1)..dim {
            let val = a[k * dim + i].abs();
            if val > max_val {
                max_val = val;
                max_row = k;
            }
        }

        if max_val < 1e-300 {
            return 0.0;
        }

        if max_row != i {
            for j in 0..dim {
                let idx1 = i * dim + j;
                let idx2 = max_row * dim + j;
                a.swap(idx1, idx2);
            }
            det = -det;
        }

        det *= a[i * dim + i];

        for k in (i + 1)..dim {
            let factor = a[k * dim + i] / a[i * dim + i];
            for j in i..dim {
                a[k * dim + j] -= factor * a[i * dim + j];
            }
        }
    }

    det
}

/// Compute the inverse of a square matrix (Gauss-Jordan elimination).
fn matrix_inverse(mat: &[f64], dim: usize) -> Option<Vec<f64>> {
    if dim == 0 {
        return Some(vec![]);
    }
    if dim == 1 {
        if mat[0].abs() < 1e-300 {
            return None;
        }
        return Some(vec![1.0 / mat[0]]);
    }

    let mut aug = vec![0.0; dim * 2 * dim];
    for i in 0..dim {
        for j in 0..dim {
            aug[i * 2 * dim + j] = mat[i * dim + j];
        }
        aug[i * 2 * dim + dim + i] = 1.0;
    }

    for i in 0..dim {
        let pivot = aug[i * 2 * dim + i];
        if pivot.abs() < 1e-300 {
            return None;
        }

        for j in 0..(2 * dim) {
            aug[i * 2 * dim + j] /= pivot;
        }

        for k in 0..dim {
            if k == i {
                continue;
            }
            let factor = aug[k * 2 * dim + i];
            for j in 0..(2 * dim) {
                aug[k * 2 * dim + j] -= factor * aug[i * 2 * dim + j];
            }
        }
    }

    let mut inv = vec![0.0; dim * dim];
    for i in 0..dim {
        for j in 0..dim {
            inv[i * dim + j] = aug[i * 2 * dim + dim + j];
        }
    }

    Some(inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmm_posterior_sums_to_one() {
        let model = GmmModel::new(2);
        let post = model.posterior(&[0.0, 0.0]);
        let sum: f64 = post.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "Posterior must sum to 1: {}", sum);
    }

    #[test]
    fn gmm_classify() {
        let model = GmmModel::new(2);
        let state = model.classify(&[0.0, 0.0]);
        assert!(matches!(
            state,
            TraderState::Noise | TraderState::Institutional | TraderState::Informed
        ));
    }

    #[test]
    fn matrix_det_2x2() {
        let mat = vec![1.0, 2.0, 3.0, 4.0];
        let det = matrix_determinant(&mat, 2);
        assert!((det - (-2.0)).abs() < 1e-9, "det={}", det);
    }

    #[test]
    fn matrix_inv_2x2() {
        let mat = vec![1.0, 2.0, 3.0, 4.0];
        let inv = matrix_inverse(&mat, 2).unwrap();
        let check = 1.0 * inv[0] + 2.0 * inv[2];
        assert!((check - 1.0).abs() < 1e-6, "check={}", check);
    }
}