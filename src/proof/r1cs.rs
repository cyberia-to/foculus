// ---
// tags: foculus, rust, zheng, ccs
// crystal-type: source
// crystal-domain: cyber
// ---
//! A rank-1 constraint list compiled to a zheng [`CCSInstance`]:
//! rows `⟨a, z⟩ · ⟨b, z⟩ = ⟨c, z⟩`, multisets `{A·B} − {C}`.
//!
//! Matrices depend only on what the caller passes in — shapes and public
//! data — never on witness values, so prover and verifier compile the same
//! instance.

use nebu::Goldilocks as F;
use zheng::types::{CCSInstance, SparseMatrix};

/// A linear form over witness columns.
pub(crate) type Lin = Vec<(usize, F)>;

/// The constant-one column as a linear form.
pub(crate) fn one() -> Lin {
    vec![(0, F::ONE)]
}

#[derive(Default)]
pub(crate) struct R1cs {
    rows: Vec<(Lin, Lin, Lin)>,
    columns: usize,
}

impl R1cs {
    /// `columns` = number of witness columns used, including z[0].
    pub fn new(columns: usize) -> Self {
        Self {
            rows: Vec::new(),
            columns,
        }
    }

    /// `a = 0`.
    pub fn zero(&mut self, a: Lin) {
        self.rows.push((a, one(), Vec::new()));
    }

    /// `x ∈ {0, 1}`: `x · x = x`.
    pub fn boolean(&mut self, x: usize) {
        self.rows
            .push((vec![(x, F::ONE)], vec![(x, F::ONE)], vec![(x, F::ONE)]));
    }

    /// `value = Σ 2^k · bits[k]` — with each bit boolean, a range check
    /// `value < 2^bits.len()` when `bits.len() < 64`.
    pub fn packs(&mut self, value: usize, bits: &[usize]) {
        let mut form = vec![(value, -F::ONE)];
        let mut weight = F::ONE;
        for &b in bits {
            self.boolean(b);
            form.push((b, weight));
            weight = weight + weight;
        }
        self.zero(form);
    }

    pub fn instance(self) -> CCSInstance {
        let num_rows = self.rows.len().max(2).next_power_of_two();
        let num_cols = self.columns.max(64).next_power_of_two();
        let mut matrices = vec![SparseMatrix::new(num_rows, num_cols); 3];
        for (r, (a, b, c)) in self.rows.into_iter().enumerate() {
            matrices[0].entries[r] = a;
            matrices[1].entries[r] = b;
            matrices[2].entries[r] = c;
        }
        CCSInstance {
            matrices,
            multisets: vec![vec![0, 1], vec![2]],
            coeffs: vec![F::ONE, -F::ONE],
            num_rows,
            num_cols,
        }
    }
}
