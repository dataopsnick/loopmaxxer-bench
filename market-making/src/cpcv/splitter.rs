//! CPCV Group Partitioning, Combination Enumeration, Purging & Embargo
//!
//! Implements the purely combinatorial half of Combinatorial Purged
//! Cross-Validation (López de Prado, *Advances in Financial Machine
//! Learning*, Ch. 12, Snippet 12.1): partition `n_samples` observations
//! into `N` contiguous groups, enumerate all `C(N, k)` test combinations,
//! purge/embargo the corresponding training set, and compute the
//! `φ = C(N-1, k-1)` backtest-path reconstruction table.

use std::collections::HashMap;
use std::ops::Range;

/// Configuration for the CPCV group partitioning.
#[derive(Debug, Clone)]
pub struct CpcvConfig {
    /// Number of contiguous groups `N` the history is partitioned into.
    pub n_groups: usize,
    /// Number of groups `k` held out as the test set in each combination.
    pub k_test_groups: usize,
    /// Number of raw event indices purged from the training set
    /// immediately *before* each test group's start boundary, removing
    /// training observations whose rolling feature-lookback window
    /// (e.g. `OrderFlowFeatures::price_history`) would otherwise overlap
    /// information contained in the test block.
    pub purge_window: usize,
    /// Fraction of total samples embargoed from the training set
    /// immediately *after* each test group's end boundary, guarding
    /// against serial-correlation leakage back into training.
    pub embargo_pct: f64,
}

impl Default for CpcvConfig {
    fn default() -> Self {
        Self {
            n_groups: 6,
            k_test_groups: 2,
            purge_window: 10,
            embargo_pct: 0.01,
        }
    }
}

/// A single purged & embargoed train/test index split.
#[derive(Debug, Clone)]
pub struct CpcvFold {
    /// Indices (into the original event slice) usable for training.
    pub train_indices: Vec<usize>,
    /// Indices (into the original event slice) held out for testing.
    pub test_indices: Vec<usize>,
}

/// Combinatorial Purged Cross-Validation splitter.
pub struct CpcvSplitter {
    config: CpcvConfig,
    n_samples: usize,
    groups: Vec<Range<usize>>,
}

impl CpcvSplitter {
    /// Create a new splitter over `n_samples` observations.
    pub fn new(config: CpcvConfig, n_samples: usize) -> Self {
        let groups = Self::partition(config.n_groups, n_samples);
        Self {
            config,
            n_samples,
            groups,
        }
    }

    /// Partition `[0, n_samples)` into `n_groups` contiguous, near-equal
    /// index ranges (any remainder is distributed one-by-one to the
    /// earliest groups).
    fn partition(n_groups: usize, n_samples: usize) -> Vec<Range<usize>> {
        let mut groups = Vec::with_capacity(n_groups);
        if n_groups == 0 || n_samples == 0 {
            return groups;
        }

        let base = n_samples / n_groups;
        let remainder = n_samples % n_groups;
        let mut start = 0usize;
        for i in 0..n_groups {
            let extra = if i < remainder { 1 } else { 0 };
            let len = base + extra;
            let end = (start + len).min(n_samples);
            groups.push(start..end);
            start = end;
        }
        groups
    }

    /// The contiguous index ranges of the `N` groups.
    pub fn groups(&self) -> &[Range<usize>] {
        &self.groups
    }

    /// Number of groups `N`.
    pub fn n_groups(&self) -> usize {
        self.groups.len()
    }

    /// All `C(N, k)` combinations of group indices chosen as the test
    /// set, generated in lexicographic order.
    pub fn combinations(&self) -> Vec<Vec<usize>> {
        combinations_of(self.groups.len(), self.config.k_test_groups)
    }

    /// The number of reconstructed, non-overlapping backtest paths:
    /// `φ[N, k] = C(N-1, k-1) = (k / N) * C(N, k)`.
    pub fn num_paths(&self) -> usize {
        let n = self.groups.len();
        let k = self.config.k_test_groups;
        if n == 0 || k == 0 || k > n {
            return 0;
        }
        binomial(n - 1, k - 1)
    }

    /// Build the purged & embargoed train/test index split for a given
    /// combination of test-group indices.
    pub fn split(&self, combo: &[usize]) -> CpcvFold {
        let test_ranges: Vec<Range<usize>> =
            combo.iter().map(|&g| self.groups[g].clone()).collect();

        let embargo_len = ((self.config.embargo_pct * self.n_samples as f64).round() as usize)
            .min(self.n_samples);

        let mut excluded = vec![false; self.n_samples];
        for r in &test_ranges {
            for i in r.clone() {
                excluded[i] = true;
            }
            // Purge: drop `purge_window` training samples immediately
            // BEFORE the test block, whose rolling lookback would
            // otherwise overlap the held-out test information.
            let purge_start = r.start.saturating_sub(self.config.purge_window);
            for i in purge_start..r.start {
                excluded[i] = true;
            }
            // Embargo: drop `embargo_len` training samples immediately
            // AFTER the test block, guarding against forward-looking
            // serial-correlation leakage.
            let embargo_end = (r.end + embargo_len).min(self.n_samples);
            for i in r.end..embargo_end {
                excluded[i] = true;
            }
        }

        let mut test_indices = Vec::new();
        for r in &test_ranges {
            test_indices.extend(r.clone());
        }
        test_indices.sort_unstable();

        let train_indices: Vec<usize> = (0..self.n_samples).filter(|i| !excluded[*i]).collect();

        CpcvFold {
            train_indices,
            test_indices,
        }
    }

    /// For every group, the ordered list of `(combination_index, path_slot)`
    /// pairs in which that group appears as a test group. Each group
    /// appears in exactly `φ` combinations, and this table assigns those
    /// φ appearances to path slots `0..φ` so that φ complete,
    /// non-overlapping backtest paths (one test-group assignment per
    /// group per path) can be reconstructed.
    pub fn path_assignment(&self) -> HashMap<usize, Vec<(usize, usize)>> {
        let combos = self.combinations();
        let n = self.groups.len();
        let mut assignment: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        let mut next_slot_for_group = vec![0usize; n];

        for (combo_idx, combo) in combos.iter().enumerate() {
            for &g in combo {
                let slot = next_slot_for_group[g];
                next_slot_for_group[g] += 1;
                assignment.entry(g).or_default().push((combo_idx, slot));
            }
        }

        assignment
    }
}

/// Advance `combo` (a strictly increasing sequence of `k` indices drawn
/// from `0..n`) to the next combination in lexicographic order. Returns
/// `false` once all combinations have been exhausted.
fn next_combination(combo: &mut [usize], n: usize) -> bool {
    let k = combo.len();
    let mut i = k;
    while i > 0 {
        i -= 1;
        if combo[i] < n - k + i {
            combo[i] += 1;
            for j in (i + 1)..k {
                combo[j] = combo[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

/// Enumerate all `C(n, k)` combinations of `{0, .., n-1}` in
/// lexicographic order.
fn combinations_of(n: usize, k: usize) -> Vec<Vec<usize>> {
    if k == 0 || k > n {
        return Vec::new();
    }

    let mut combo: Vec<usize> = (0..k).collect();
    let mut result = vec![combo.clone()];
    while next_combination(&mut combo, n) {
        result.push(combo.clone());
    }
    result
}

/// Compute the binomial coefficient `C(n, k)` using `u128` intermediate
/// precision to avoid overflow for the group counts used in practice.
fn binomial(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result: u128 = 1;
    for i in 0..k {
        result = result * (n - i) as u128 / (i + 1) as u128;
    }
    result as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combination_count_matches_binomial() {
        let combos = combinations_of(6, 2);
        assert_eq!(combos.len(), 15, "C(6,2) should be 15");

        let combos = combinations_of(5, 3);
        assert_eq!(combos.len(), 10, "C(5,3) should be 10");
    }

    #[test]
    fn combinations_are_sorted_and_unique() {
        let combos = combinations_of(5, 2);
        let mut seen = std::collections::HashSet::new();
        for combo in &combos {
            assert!(
                combo.windows(2).all(|w| w[0] < w[1]),
                "combo must be strictly increasing"
            );
            assert!(seen.insert(combo.clone()), "combo must be unique");
        }
        assert_eq!(combos.len(), 10);
    }

    #[test]
    fn num_paths_matches_two_equivalent_formulas() {
        for (n, k) in [(6usize, 2usize), (5, 3), (10, 4), (8, 1), (9, 9)] {
            let splitter = CpcvSplitter::new(
                CpcvConfig {
                    n_groups: n,
                    k_test_groups: k,
                    purge_window: 0,
                    embargo_pct: 0.0,
                },
                1000,
            );
            let phi = splitter.num_paths();
            let c_nk = binomial(n, k);
            // φ[N,k] = C(N-1,k-1) = (k/N) * C(N,k)
            let phi_alt = (k * c_nk) / n;
            assert_eq!(phi, phi_alt, "phi formulas disagree for N={}, k={}", n, k);
        }
    }

    #[test]
    fn every_group_appears_in_exactly_phi_combinations() {
        let n = 6;
        let k = 2;
        let splitter = CpcvSplitter::new(
            CpcvConfig {
                n_groups: n,
                k_test_groups: k,
                purge_window: 0,
                embargo_pct: 0.0,
            },
            1200,
        );
        let phi = splitter.num_paths();
        let assignment = splitter.path_assignment();

        for g in 0..n {
            let appearances = assignment.get(&g).expect("group must appear at least once");
            assert_eq!(
                appearances.len(),
                phi,
                "group {} should appear in exactly phi={} combinations",
                g,
                phi
            );
            let mut slots: Vec<usize> = appearances.iter().map(|(_, slot)| *slot).collect();
            slots.sort_unstable();
            assert_eq!(slots, (0..phi).collect::<Vec<_>>());
        }
    }

    #[test]
    fn train_and_test_indices_never_overlap() {
        let splitter = CpcvSplitter::new(
            CpcvConfig {
                n_groups: 6,
                k_test_groups: 2,
                purge_window: 5,
                embargo_pct: 0.02,
            },
            1200,
        );

        for combo in splitter.combinations() {
            let fold = splitter.split(&combo);
            let train_set: std::collections::HashSet<usize> =
                fold.train_indices.iter().copied().collect();
            for idx in &fold.test_indices {
                assert!(
                    !train_set.contains(idx),
                    "index {} present in both train and test sets",
                    idx
                );
            }
        }
    }

    #[test]
    fn purge_and_embargo_shrink_training_set() {
        let n_samples = 1200;
        let combo = vec![2usize];

        let no_purge = CpcvSplitter::new(
            CpcvConfig {
                n_groups: 6,
                k_test_groups: 1,
                purge_window: 0,
                embargo_pct: 0.0,
            },
            n_samples,
        );
        let with_purge = CpcvSplitter::new(
            CpcvConfig {
                n_groups: 6,
                k_test_groups: 1,
                purge_window: 20,
                embargo_pct: 0.05,
            },
            n_samples,
        );

        let fold_no_purge = no_purge.split(&combo);
        let fold_with_purge = with_purge.split(&combo);

        assert!(
            fold_with_purge.train_indices.len() < fold_no_purge.train_indices.len(),
            "purge/embargo should strictly reduce the training set size"
        );
    }

    #[test]
    fn groups_partition_all_samples_exactly_once() {
        let splitter = CpcvSplitter::new(CpcvConfig::default(), 1007);
        let mut covered = vec![false; 1007];
        for range in splitter.groups() {
            for i in range.clone() {
                assert!(!covered[i], "index {} covered by more than one group", i);
                covered[i] = true;
            }
        }
        assert!(
            covered.iter().all(|&c| c),
            "every index must belong to some group"
        );
    }
}
