// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Unit checks of the inverse 3D LUT's range tree. Upstream has no tests of the tree itself;
//! these check the structure its construction implies (`RangeTree::initialize`,
//! src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1306-1388 @ v2.5.2): one leaf per cube of the
//! grid, ordered by the interleaved bits of its indices, each node holding the cubes whose
//! indices share its level's leading bits, and each node's range being its cubes' corner
//! values, widened by `1e-6`. The renderer's results are checked against the wheel
//! (`tests/lut3d_oracle.rs`).

use std::collections::BTreeSet;

use ocio_testkit::probe::Rng;

use super::*;

/// Grid sizes of the extrapolated LUTs the renderer builds trees for (a LUT of `n` entries per
/// side becomes `n + 2`): the smallest, each side of the powers of two that add a level, and
/// the largest (129 + 2).
const GRID_SIZES: [usize; 14] = [4, 5, 6, 9, 10, 11, 17, 18, 19, 34, 35, 66, 67, 131];

/// A blue-fastest grid of `gsz^3` RGB entries.
fn grid(gsz: usize, mut value: impl FnMut(usize, usize, usize, usize) -> f32) -> Vec<f32> {
    let mut values = Vec::with_capacity(gsz * gsz * gsz * 3);
    for i in 0..gsz {
        for j in 0..gsz {
            for k in 0..gsz {
                for c in 0..3 {
                    values.push(value(i, j, k, c));
                }
            }
        }
    }
    values
}

fn identity_grid(gsz: usize) -> Vec<f32> {
    let step = 1.0f32 / (gsz as f32 - 1.0f32);
    grid(gsz, |i, j, k, c| [i, j, k][c] as f32 * step)
}

fn random_grid(gsz: usize, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    grid(gsz, |_, _, _, _| rng.uniform(-0.5, 1.5))
}

/// The number of bits of `v`.
fn bit_length(v: usize) -> usize {
    (usize::BITS - v.leading_zeros()) as usize
}

/// Checks the tree of `values` (a grid of `gsz` entries per side) against the structure its
/// construction implies.
fn check_tree(values: &[f32], gsz: usize) {
    let mut tree = RangeTree::new();
    tree.initialize(values, gsz);

    assert_eq!(tree.get_chans(), 3);
    assert_eq!(tree.get_grid_size(), &[gsz, gsz, gsz, 0]);
    let depth = tree.get_depth();
    assert_eq!(depth, bit_length(gsz - 2), "gsz {gsz}");
    let levels = tree.get_levels();
    assert_eq!(levels.len(), depth);

    // One leaf per cube, each cube once, in increasing hash order.
    let cubes = gsz - 1;
    let base_inds = tree.get_base_inds();
    assert_eq!(base_inds.len(), cubes * cubes * cubes);
    assert_eq!(levels[depth - 1].elems, base_inds.len());
    let distinct: BTreeSet<[usize; 3]> = base_inds.iter().map(|b| b.inds).collect();
    assert_eq!(distinct.len(), base_inds.len());
    assert!(base_inds.iter().all(|b| b.inds.iter().all(|&x| x < cubes)));
    assert!(base_inds.windows(2).all(|w| w[0].hash < w[1].hash));

    // Each level's nodes partition the next level, with at most 8 children each.
    for level in 0..depth - 1 {
        let this = &levels[level];
        assert_eq!(this.chans, 3);
        assert_eq!(this.child0_offsets.len(), this.elems);
        assert_eq!(this.num_children.len(), this.elems);
        assert_eq!(this.child0_offsets[0], 0);
        for i in 0..this.elems {
            assert!(
                (1..=8).contains(&this.num_children[i]),
                "level {level} node {i}"
            );
            if i + 1 < this.elems {
                assert_eq!(
                    this.child0_offsets[i + 1],
                    this.child0_offsets[i] + this.num_children[i]
                );
            }
        }
        assert_eq!(
            this.child0_offsets[this.elems - 1] + this.num_children[this.elems - 1],
            levels[level + 1].elems
        );
    }

    // The leaves each node holds: their indices share the node's leading bits, and other
    // nodes of its level hold other leading bits.
    let mut leaves_of: Vec<Vec<std::ops::Range<usize>>> = vec![Vec::new(); depth];
    leaves_of[depth - 1] = (0..base_inds.len()).map(|i| i..i + 1).collect();
    for level in (0..depth - 1).rev() {
        let this = &levels[level];
        let below = &leaves_of[level + 1];
        leaves_of[level] = (0..this.elems)
            .map(|i| {
                let first = this.child0_offsets[i];
                let last = first + this.num_children[i] - 1;
                below[first].start..below[last].end
            })
            .collect();
    }
    for (level, nodes) in leaves_of.iter().enumerate() {
        let shift = depth - 1 - level;
        let mut prefixes = BTreeSet::new();
        assert_eq!(nodes.len(), levels[level].elems);
        for node in nodes {
            let prefix = base_inds[node.start].inds.map(|x| x >> shift);
            assert!(
                base_inds[node.clone()]
                    .iter()
                    .all(|b| b.inds.map(|x| x >> shift) == prefix),
                "level {level}"
            );
            assert!(prefixes.insert(prefix), "level {level}: prefix twice");
        }
    }

    // Each node's range: its cubes' corner values, widened by 1e-6.
    let corner = |b: &BaseInd, c: usize, d: [usize; 3]| {
        let [i, j, k] = b.inds;
        values[((i + d[0]) * gsz * gsz + (j + d[1]) * gsz + (k + d[2])) * 3 + c]
    };
    let mut deltas = Vec::new();
    for di in 0..2 {
        for dj in 0..2 {
            for dk in 0..2 {
                deltas.push([di, dj, dk]);
            }
        }
    }
    for (level, nodes) in leaves_of.iter().enumerate() {
        for (n, node) in nodes.iter().enumerate() {
            for c in 0..3 {
                let corners = base_inds[node.clone()]
                    .iter()
                    .flat_map(|b| deltas.iter().map(move |&d| corner(b, c, d)));
                let lo = corners.clone().fold(f32::INFINITY, f32::min);
                let hi = corners.fold(f32::NEG_INFINITY, f32::max);
                assert_eq!(
                    levels[level].min_vals[n * 3 + c].to_bits(),
                    (lo - 1e-6f32).to_bits(),
                    "level {level} node {n} channel {c}"
                );
                assert_eq!(
                    levels[level].max_vals[n * 3 + c].to_bits(),
                    (hi + 1e-6f32).to_bits(),
                    "level {level} node {n} channel {c}"
                );
            }
        }
    }
}

#[test]
fn identity_grids_build_the_implied_tree() {
    for gsz in GRID_SIZES {
        check_tree(&identity_grid(gsz), gsz);
    }
}

#[test]
fn random_grids_build_the_implied_tree() {
    for (seed, gsz) in GRID_SIZES.into_iter().enumerate() {
        check_tree(&random_grid(gsz, 0x1a7_3d00 + seed as u64), gsz);
    }
}

/// `frexp_exponent` against the C runtime's `frexp`, on every value the renderer's grid
/// sizes give it and beyond.
#[test]
fn frexp_exponent_matches_the_c_runtime() {
    for v in 1..=1024u32 {
        let x = v as f32;
        assert_eq!(
            frexp_exponent(x),
            ocio_testkit::crt::frexp_c(f64::from(x)).1,
            "{v}"
        );
    }
}
