// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exact inverse of a 3D LUT: a port of `InvLut3DRenderer` and its `RangeTree`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:78-196 and 1080-1444 @ v2.5.2).
//!
//! **Integer widths.** Upstream counts and indexes in `unsigned long` (32 bits on Windows, 64
//! on Linux). Here they are `usize`. The renderer builds the tree on the LUT extrapolated by
//! one entry per side, so the grid has at most 131 entries per side and the tree at most 8
//! levels: every count, offset and hash then stays below 2^31, and both widths give the same
//! values. (The extrapolated LUT is itself a 3D LUT, so in fact at most 129 entries per side:
//! I-153.)
//!
//! **Arithmetic.** The tree's ranges are `float`, compared with C++'s `std::min` and
//! `std::max` ([`std_min`], [`std_max`]). `invert_hypercube` computes in `double`, in
//! upstream's order. Its NaNs never reach an output: a NaN inverse fails no feasibility test,
//! so the walk ends with it, and the final `Clamp` turns it into 0.

use std::ffi::c_ulong;

use super::lut3d_op_data::{Lut3DArray, Lut3DOpData};
use crate::exception::{Exception, Result};
use crate::math_utils::{clamp, std_max, std_min};
use crate::op::CpuOp;

/// The most input and output channels upstream sizes its scratch arrays for (`MAX_N`).
const MAX_N: usize = 4;

/// A level of the [`RangeTree`]. Port of `InvLut3DRenderer::treeLevel`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:81-94 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct TreeLevel {
    /// `elems`: the number of nodes on this level.
    pub elems: usize,
    /// `chans`: the LUT's input and output channels.
    pub chans: usize,
    /// `minVals`: each node's smallest LUT value per channel, over its sub-tree.
    pub min_vals: Vec<f32>,
    /// `maxVals`: each node's largest LUT value per channel, over its sub-tree.
    pub max_vals: Vec<f32>,
    /// `child0offsets`: the index of each node's first child on the next level.
    pub child0_offsets: Vec<usize>,
    /// `numChildren`: each node's number of children.
    pub num_children: Vec<usize>,
}

/// The base grid entry of a cube of the LUT. Port of `InvLut3DRenderer::baseInd`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:97-107 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BaseInd {
    /// `inds`: the indices into the LUT.
    pub inds: [usize; 3],
    /// `hash`: the cube's position in the tree.
    pub hash: usize,
}

/// The exponent `frexp` returns for a positive normal `x`: the `e` of `x = m * 2^e` with
/// `m` in `[0.5, 1)`.
fn frexp_exponent(x: f32) -> i32 {
    debug_assert!(x.is_normal() && x > 0.0);
    ((x.to_bits() >> 23) & 0xff) as i32 - 126
}

/// A tree for fast range queries in a LUT. Since LUT interpolation is a convex operation, the
/// output must be between the min and max value for each channel; this modified nd-tree
/// identifies the cubes of the LUT that could contain the inverse.
///
/// Port of `InvLut3DRenderer::RangeTree` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:110-160
/// and 1080-1388 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct RangeTree {
    /// `m_chans`
    chans: usize,
    /// `m_gsz`
    gsz: [usize; 4],
    /// `m_depth`
    depth: usize,
    /// `m_levels`
    levels: Vec<TreeLevel>,
    /// `m_baseInds`
    base_inds: Vec<BaseInd>,
    /// `m_levelScales`
    level_scales: Vec<usize>,
}

impl RangeTree {
    /// An empty tree. Port of `RangeTree::RangeTree` (Lut3DOpCPU.cpp:1080-1082 @ v2.5.2).
    pub fn new() -> RangeTree {
        RangeTree::default()
    }

    /// The number of input and output channels. Port of `RangeTree::getChans`
    /// (Lut3DOpCPU.cpp:130 @ v2.5.2).
    pub fn get_chans(&self) -> usize {
        self.chans
    }

    /// The length of each side of the LUT. Port of `RangeTree::getGridSize`
    /// (Lut3DOpCPU.cpp:133 @ v2.5.2).
    pub fn get_grid_size(&self) -> &[usize; 4] {
        &self.gsz
    }

    /// The number of levels. Port of `RangeTree::getDepth` (Lut3DOpCPU.cpp:136 @ v2.5.2).
    pub fn get_depth(&self) -> usize {
        self.depth
    }

    /// The levels, the root first. Port of `RangeTree::getLevels` (Lut3DOpCPU.cpp:139 @ v2.5.2).
    pub fn get_levels(&self) -> &[TreeLevel] {
        &self.levels
    }

    /// The base entries of the LUT's cubes, in the order of the last level. Port of
    /// `RangeTree::getBaseInds` (Lut3DOpCPU.cpp:142 @ v2.5.2).
    pub fn get_base_inds(&self) -> &[BaseInd] {
        &self.base_inds
    }

    /// The last level's ranges: each cube's smallest and largest value per channel, widened by
    /// `1e-6`.
    ///
    /// Port of `RangeTree::initRanges` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1088-1157
    /// @ v2.5.2). Upstream throws "Unsupported channel number." for other than 2 or 3
    /// channels; [`RangeTree::initialize`] always sets 3.
    fn init_ranges(&mut self, grvec: &[f32]) {
        let chans = self.chans;
        let gsz = self.gsz;
        let depthm1 = self.depth - 1;
        let n = self.levels[depthm1].elems;
        self.levels[depthm1].min_vals.resize(n * chans, 0.0);
        self.levels[depthm1].max_vals.resize(n * chans, 0.0);
        // Our 3d-LUTs are stored with the blue chan varying most rapidly.
        let ind0scale = gsz[2] * gsz[1];
        let ind1scale = gsz[2];
        let mut corner_offsets = [0usize; 8];
        let corners;

        if chans == 3 {
            corners = 8;
            corner_offsets[0] = 0; // base
            corner_offsets[1] = 1; // increment along B
            corner_offsets[2] = gsz[2]; // increment along G
            corner_offsets[3] = gsz[2] + 1; // increment along B + G
            corner_offsets[4] = gsz[2] * gsz[1]; // increment along R
            corner_offsets[5] = gsz[2] * gsz[1] + 1; // increment along B + R
            corner_offsets[6] = gsz[2] * gsz[1] + gsz[2]; // increment along R + G
            corner_offsets[7] = gsz[2] * gsz[1] + gsz[2] + 1; // increment along B + G + R
        } else if chans == 2 {
            corners = 4;
            corner_offsets[0] = 0; // base
            corner_offsets[1] = 1; // increment along X
            corner_offsets[2] = gsz[1]; // increment along Y
            corner_offsets[3] = gsz[1] + 1; // increment along X + Y
        } else {
            unreachable!("RangeTree::initialize always sets 3 channels");
        }

        let mut min_val = [0.0f32; MAX_N];
        let mut max_val = [0.0f32; MAX_N];
        for i in 0..n {
            let inds = self.base_inds[i].inds;
            let base_offset = inds[0] * ind0scale + inds[1] * ind1scale + inds[2];

            for k in 0..chans {
                min_val[k] = grvec[base_offset * chans + k];
                max_val[k] = min_val[k];
            }

            for offset in corner_offsets.iter().take(corners).skip(1) {
                let index = (base_offset + offset) * chans;
                for k in 0..chans {
                    min_val[k] = std_min(min_val[k], grvec[index + k]);
                    max_val[k] = std_max(max_val[k], grvec[index + k]);
                }
            }

            // Expand the ranges slightly to allow for error in forward evaluation.
            const TOL: f32 = 1e-6f32;

            let level = &mut self.levels[depthm1];
            for k in 0..chans {
                level.min_vals[i * chans + k] = min_val[k] - TOL;
                level.max_vals[i * chans + k] = max_val[k] + TOL;
            }
        }
    }

    /// One base entry per cube of the LUT, red slowest and blue fastest.
    ///
    /// Port of `RangeTree::initInds` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1159-1202
    /// @ v2.5.2).
    fn init_inds(&mut self) {
        if self.chans == 3 {
            let i_lim = self.gsz[0] - 1;
            let j_lim = self.gsz[1] - 1;
            let k_lim = self.gsz[2] - 1;

            self.base_inds
                .resize(i_lim * j_lim * k_lim, BaseInd::default());

            let mut cnt = 0;
            for i in 0..i_lim {
                for j in 0..j_lim {
                    for k in 0..k_lim {
                        self.base_inds[cnt].inds[0] = i;
                        self.base_inds[cnt].inds[1] = j;
                        self.base_inds[cnt].inds[2] = k;
                        cnt += 1;
                    }
                }
            }
        } else if self.chans == 2 {
            let i_lim = self.gsz[0] - 1;
            let j_lim = self.gsz[1] - 1;

            self.base_inds.resize(i_lim * j_lim, BaseInd::default());

            let mut cnt = 0;
            for i in 0..i_lim {
                for j in 0..j_lim {
                    self.base_inds[cnt].inds[0] = i;
                    self.base_inds[cnt].inds[1] = j;
                    cnt += 1;
                }
            }
        }
    }

    /// The hash of base entry `i`: one base-16 digit per level, the root's the most
    /// significant, holding that level's bit of each index.
    ///
    /// Port of `RangeTree::indsToHash` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1204-1225
    /// @ v2.5.2).
    fn inds_to_hash(&mut self, i: usize) {
        const POWS2: [usize; 4] = [1, 2, 4, 8];
        let mut key_bits = [0usize; 16];
        let depthm1 = self.depth - 1;

        let inds = self.base_inds[i].inds;
        for (level, bits) in key_bits.iter_mut().enumerate().take(self.depth) {
            *bits = 0;
            for (ch, pow2) in POWS2.iter().enumerate().take(self.chans) {
                let ind_bit = (inds[ch] >> (depthm1 - level)) & 1;
                *bits += ind_bit * pow2;
            }
        }
        let mut hash = 0;
        for (bits, scale) in key_bits.iter().zip(&self.level_scales).take(self.depth) {
            hash += bits * scale;
        }
        self.base_inds[i].hash = hash;
    }

    /// The children of each node of `level`: a new node starts where consecutive hashes of
    /// the next level are more than a full set of children apart.
    ///
    /// Port of `RangeTree::updateChildren` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1227-1259
    /// @ v2.5.2).
    fn update_children(&mut self, hashes: &[usize], level: usize) {
        let level_size = self.levels[level].elems;
        let max_children = 1usize << self.chans;
        let gap = self.level_scales[level + 1] * max_children;
        let tree_level = &mut self.levels[level];
        tree_level.child0_offsets.resize(level_size, 0);
        tree_level.num_children.resize(level_size, 0);

        let mut cnt = 1;

        tree_level.child0_offsets[0] = 0;
        let prev_size = hashes.len();
        for i in 1..prev_size {
            if hashes[i] - hashes[i - 1] > gap {
                tree_level.child0_offsets[cnt] = i;
                cnt += 1;
            }
        }

        for i in 0..level_size - 1 {
            tree_level.num_children[i] =
                tree_level.child0_offsets[i + 1] - tree_level.child0_offsets[i];
        }
        let tmp = hashes.len() - tree_level.child0_offsets[level_size - 1];
        tree_level.num_children[level_size - 1] = tmp;
    }

    /// The ranges of `level`'s nodes: those of their children combined.
    ///
    /// Port of `RangeTree::updateRanges` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1261-1304
    /// @ v2.5.2).
    fn update_ranges(&mut self, level: usize) {
        let chans = self.chans;
        let max_children = 1usize << chans;
        let level_size = self.levels[level].elems;
        let (upper, lower) = self.levels.split_at_mut(level + 1);
        let this = &mut upper[level];
        let next = &lower[0];

        this.min_vals.resize(level_size * chans, 0.0);
        this.max_vals.resize(level_size * chans, 0.0);

        for i in 0..level_size {
            let index = this.child0_offsets[i];
            for k in 0..chans {
                this.min_vals[i * chans + k] = next.min_vals[index * chans + k];
                this.max_vals[i * chans + k] = next.max_vals[index * chans + k];
            }

            // New min/max combine the min/max for all children from next lower level.
            for j in 2..=max_children {
                if this.num_children[i] >= j {
                    let ind = index + j - 1;
                    for k in 0..chans {
                        let min_val = this.min_vals[i * chans + k];
                        let child_min_val = next.min_vals[ind * chans + k];
                        if child_min_val < min_val {
                            this.min_vals[i * chans + k] = child_min_val;
                        }
                        let max_val = this.max_vals[i * chans + k];
                        let child_max_val = next.max_vals[ind * chans + k];
                        if child_max_val > max_val {
                            this.max_vals[i * chans + k] = child_max_val;
                        }
                    }
                }
            }
        }
    }

    /// Builds the tree of a 3D LUT of `gsz` entries per side, `grvec` in blue-fastest order.
    /// The renderer calls it with the extrapolated LUT, so `gsz` is at least 4 (a grid of 3
    /// would give a tree of one level, 2 or less none).
    ///
    /// Port of `RangeTree::initialize` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1306-1388
    /// @ v2.5.2).
    pub fn initialize(&mut self, grvec: &[f32], gsz: usize) {
        self.chans = 3; // only supporting Lut3D for now
        self.gsz = [gsz, gsz, gsz, 0];

        // Determine depth of tree.
        let mut max_gsz = 0.0f32;
        for i in 0..self.chans {
            max_gsz = std_max(max_gsz, self.gsz[i] as f32);
        }
        let log2base = frexp_exponent(max_gsz - 2.0f32);
        self.depth = log2base as usize;

        self.levels.resize(self.depth, TreeLevel::default());

        // Determine size of each level.
        for i in 0..self.depth {
            let mut level_size = 1;
            for j in 0..self.chans {
                let g = self.gsz[j] - 2;
                let m = g >> (self.depth - 1 - i);
                level_size *= m + 1;
            }
            self.levels[i].elems = level_size;
            self.levels[i].chans = self.chans;
        }

        // Determine scale to use for hash.
        self.level_scales.resize(self.depth, 0);
        for level in 0..self.depth {
            let depthm1 = self.depth - 1;
            let shift = (self.chans + 1) * (depthm1 - level);
            let scale = 1usize << shift;
            self.level_scales[level] = scale;
        }

        // Initialize indices into 3d-LUT.
        self.init_inds();

        // Calculate hash for indices.
        let cnt = self.base_inds.len();
        for i in 0..cnt {
            self.inds_to_hash(i);
        }

        // Sort indices based on hash. (`std::sort`: the hashes are distinct, one per cube, so
        // every sort gives the same order.)
        self.base_inds.sort_unstable_by_key(|b| b.hash);

        // Copy sorted hashes into temp vector.
        let mut hashes: Vec<usize> = self.base_inds.iter().map(|b| b.hash).collect();

        // Initialize min/max ranges from LUT entries.
        self.init_ranges(grvec);

        // Start at bottom of tree and work up, consolidating levels.
        for level in (0..self.depth.saturating_sub(1)).rev() {
            self.update_children(&hashes, level);

            let level_size = self.levels[level].elems;

            for i in 0..level_size {
                let index = self.levels[level].child0_offsets[i];
                hashes[i] = hashes[index];
            }
            hashes.truncate(level_size);

            self.update_ranges(level);
        }
    }
}

// The inversion code is based on an algorithm in "Numerical Linear Algebra and Optimization,
// vol. 1," by Gill, Murray, and Wright.

/// The longest program of factorization sweeps (`MAX_SWEEPS`). The renderer's program can't
/// reach it: its first step adds no sweep, its second at most one, and each of its six others
/// at most three, so a call adds at most 19.
const MAX_SWEEPS: usize = 20;

/// Whether the cube of the LUT `gr` whose base entry is `guess` contains the inverse of `val`:
/// a customized matrix factorization updating technique, walking the cube's tetrahedra along
/// the program `ops_list`, `entering_list`, `new_vert_list`, `path_list` and `path_order`. On
/// success, `x_out` gets the inverse in index units; otherwise it is left alone.
///
/// Every step computes in `double`, as upstream does: the LUT values and `val` are widened,
/// the inverse narrowed to `float` per channel.
///
/// Port of `invert_hypercube` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:822-1078 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn invert_hypercube(
    n: usize,
    x_out: &mut [f32; 3],
    gr: &[f32],
    ind2off: &[usize; 3],
    val: &[f32; 3],
    guess: &[usize; 3],
    list_len: usize,
    ops_list: &[i64],
    entering_list: &[usize],
    new_vert_list: &[usize],
    path_list: &[usize],
    path_order: &[usize],
) -> bool {
    // Singularity tolerance
    const ZERO_TOL: f64 = 1.0e-9;
    // Feasibility tolerances
    const NEGZERO_TOL: f64 = -1.0e-9;
    const ONE_TOL: f64 = 1.0 + 1.0e-9;

    let mut row_perm = [0usize; MAX_N];
    let mut col_perm = [0usize; MAX_N];
    let mut sweep_to = [0usize; MAX_SWEEPS];
    let mut sweep_from = [0usize; MAX_SWEEPS];
    let mut base_vert = [0.0f64; MAX_N];
    let mut y = [0.0f64; MAX_N];
    let mut u = [[0.0f64; MAX_N]; MAX_N];
    let mut x = [0.0f64; MAX_N];
    let mut sweep_f = [0.0f64; MAX_SWEEPS];
    let mut b = [0.0f64; MAX_N];
    let mut x2 = [0.0f64; MAX_N];
    let mut new_vert = [0.0f64; MAX_N];

    let mut backsub: i64;
    let mut infeas = false;
    let nm1 = n - 1;
    let nm2 = n - 2;
    let mut numsweeps = 0usize;

    let mut base_ind = 0usize;
    for i in 0..n {
        base_ind += guess[i] * ind2off[i];
    }

    for i in 0..n {
        row_perm[i] = i;
        col_perm[i] = i;
        base_vert[i] = f64::from(gr[base_ind + i]);
        b[i] = f64::from(val[i]) - base_vert[i];
        y[i] = b[i];
        for (j, v) in u[i].iter_mut().enumerate().take(n) {
            *v = if i == j { 1.0 } else { 0.0 };
        }
    }

    for i in 0..list_len {
        backsub = ops_list[i];
        if backsub < 0 {
            numsweeps = 0;
            backsub = 0;
            for j in 0..n {
                y[j] = b[j];
                row_perm[j] = j;
                col_perm[j] = j;
                for (k, v) in u[j].iter_mut().enumerate().take(n) {
                    *v = if j == k { 1.0 } else { 0.0 };
                }
            }
        }

        let entering_ind = entering_list[i];
        for j in 0..n {
            let tmp_ind = base_ind + n * new_vert_list[i];
            new_vert[j] = f64::from(gr[tmp_ind + j]) - base_vert[j];
        }

        for j in 0..numsweeps {
            new_vert[sweep_to[j]] -= sweep_f[j] * new_vert[sweep_from[j]];
        }

        let mut leaving_nz = 0usize;

        for (j, row) in u.iter_mut().enumerate().take(n) {
            row[entering_ind] = new_vert[j];
            if col_perm[j] == entering_ind {
                leaving_nz = j + 1;
            }
        }

        if leaving_nz <= nm2 {
            let tmp_ind = col_perm[leaving_nz - 1];
            for j in leaving_nz - 1..nm2 {
                col_perm[j] = col_perm[j + 1];
            }
            col_perm[nm2] = tmp_ind;
        }

        for j in leaving_nz - 1..nm1 {
            let jp1 = j + 1;
            let mut piv = j;
            let mut col_piv = j;
            let mut abs_d = u[row_perm[j]][col_perm[j]].abs();
            for (k, &row) in row_perm.iter().enumerate().take(n).skip(jp1) {
                let abs_n = u[row][col_perm[j]].abs();
                if abs_n > abs_d {
                    abs_d = abs_n;
                    piv = k;
                }
            }

            if abs_d < ZERO_TOL {
                // (decided to always do rank revealing factorization here, slower but more
                // robust)
                for h in jp1..n {
                    for k in j..n {
                        let abs_n = u[row_perm[k]][col_perm[h]].abs();
                        if abs_n > abs_d {
                            abs_d = abs_n;
                            piv = k;
                            col_piv = h;
                        }
                    }
                    if abs_d > ZERO_TOL {
                        col_perm.swap(j, col_piv);
                    }
                }
            }
            if piv != j {
                row_perm.swap(j, piv);
            }

            let denom = u[row_perm[j]][col_perm[j]];
            for h in jp1..n {
                let num = u[row_perm[h]][col_perm[j]];
                if num.abs() >= ZERO_TOL {
                    let f = num / denom;
                    u[row_perm[h]][col_perm[j]] = 0.0;
                    for k in jp1..n {
                        u[row_perm[h]][col_perm[k]] -= f * u[row_perm[j]][col_perm[k]];
                    }
                    y[row_perm[h]] -= f * y[row_perm[j]];
                    sweep_to[numsweeps] = row_perm[h];
                    sweep_from[numsweeps] = row_perm[j];
                    sweep_f[numsweeps] = f;
                    numsweeps += 1;
                }
            }
        }

        if backsub != 0 {
            let mut running_sumx = 0.0f64;
            for js in (0..n).rev() {
                let denom = u[row_perm[js]][col_perm[js]];
                if denom.abs() < ZERO_TOL {
                    if y[row_perm[js]].abs() > ZERO_TOL {
                        infeas = true;
                        break;
                    } else {
                        x[js] = 0.0;
                        infeas = false;
                    }
                } else {
                    let mut sm = 0.0f64;
                    for k in js + 1..n {
                        sm += u[row_perm[js]][col_perm[k]] * x[k];
                    }
                    let x_tmp = (y[row_perm[js]] - sm) / denom;

                    infeas = x_tmp < NEGZERO_TOL;
                    if infeas {
                        break;
                    }
                    running_sumx += x_tmp;
                    infeas = running_sumx > ONE_TOL;
                    if infeas {
                        break;
                    }

                    x[js] = x_tmp;
                }
            }

            if !infeas {
                for j in 0..n {
                    x2[col_perm[j]] = x[j];
                }

                let mut tmp_ind = i * n + n - 1;
                x_out[path_list[tmp_ind]] = x2[path_order[0]] as f32;
                tmp_ind = tmp_ind.wrapping_sub(1);
                for j in 1..n {
                    x_out[path_list[tmp_ind]] =
                        (x2[path_order[j]] + f64::from(x_out[path_list[tmp_ind + 1]])) as f32;
                    tmp_ind = tmp_ind.wrapping_sub(1);
                }

                break;
            }
        }
    }

    if infeas {
        false
    } else {
        for j in 0..n {
            x_out[j] += guess[j] as f32;
        }
        true
    }
}

/// Moves `rgb` away from `center` by `scale`. Port of `extrapolate`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1424-1430 @ v2.5.2).
fn extrapolate(rgb: [f32; 3], center: f32, scale: f32) -> [f32; 3] {
    [
        (rgb[0] - center) * scale + center,
        (rgb[1] - center) * scale + center,
        (rgb[2] - center) * scale + center,
    ]
}

/// The error for a grid size of 1, where upstream's extrapolation never ends (U-65).
pub const GRID_SIZE_1_INVERSE: &str =
    "Lut3D: the exact inverse of a 3D LUT needs a grid size of at least 2.";

/// The exact inverse of a 3D LUT, assuming tetrahedral interpolation in the forward direction.
/// Values outside the range of the forward LUT are clamped to someplace on the exterior
/// surface of the LUT.
///
/// Port of `InvLut3DRenderer` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:162-194 and
/// 1432-1734 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct InvLut3DRenderer {
    /// `m_scale`: the output scaling for r, g and b.
    scale: f32,
    /// `m_dim`: the grid size of the extrapolated LUT.
    dim: i64,
    /// `m_tree`: the range tree of the extrapolated LUT.
    tree: RangeTree,
    /// `m_grvec`: the extrapolated LUT.
    grvec: Vec<f32>,
}

impl InvLut3DRenderer {
    /// The renderer of the inverse of `lut`: "LUT 3D: Grid size '<n>' must not be greater
    /// than '129'." for a grid size of 128 or 129, whose extrapolated LUT is over the limit
    /// (I-153); [`GRID_SIZE_1_INVERSE`] for a grid size of 1 (U-65).
    ///
    /// Port of `InvLut3DRenderer::InvLut3DRenderer` (Lut3DOpCPU.cpp:1432-1439 @ v2.5.2).
    pub fn new(lut: &Lut3DOpData) -> Result<InvLut3DRenderer> {
        let mut renderer = InvLut3DRenderer {
            scale: 0.0,
            dim: 0,
            tree: RangeTree::new(),
            grvec: Vec::new(),
        };
        renderer.update_data(lut)?;
        Ok(renderer)
    }

    /// Port of `InvLut3DRenderer::updateData`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1445-1459 @ v2.5.2).
    fn update_data(&mut self, lut: &Lut3DOpData) -> Result<()> {
        self.extrapolate_3d_array(lut)?;

        // extrapolation adds 2 (to a grid size of at most 127 here)
        self.dim = lut.get_array().get_length() as i64 + 2;

        self.tree.initialize(&self.grvec, self.dim as usize);

        // Converts from index units to inDepth units of the original LUT.
        // (Note that inDepth of the original LUT is outDepth of the inverse LUT.)
        // (Note that the result should be relative to the unextrapolated LUT,
        //  hence the dim - 3.)
        self.scale = 1.0f32 / (self.dim - 3) as f32;
        Ok(())
    }

    /// The LUT grown by one entry on each side, each new entry its neighbor moved away from
    /// 0.5 by 4, to handle values outside the LUT gamut.
    ///
    /// Port of `InvLut3DRenderer::extrapolate3DArray`
    /// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1461-1591 @ v2.5.2). Upstream's loops over
    /// the two ends of a side step by `dim - 1`; for a grid size of 1 that is 0, and they
    /// never end: the port returns [`GRID_SIZE_1_INVERSE`] instead (U-65). The six groups of
    /// faces, edges and corners write disjoint entries.
    fn extrapolate_3d_array(&mut self, lut: &Lut3DOpData) -> Result<()> {
        let dim = lut.get_array().get_length() as usize;
        if dim == 1 {
            return Err(Exception::new(GRID_SIZE_1_INVERSE));
        }
        let new_dim = dim + 2;

        let array = lut.get_array();

        let mut new_array = Lut3DArray::new(new_dim as c_ulong)?;

        // Copy center values.
        for idx in 0..dim {
            for jdx in 0..dim {
                for kdx in 0..dim {
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    new_array.set_rgb(idx + 1, jdx + 1, kdx + 1, rgb);
                }
            }
        }

        let center = 0.5f32;
        let scale = 4.0f32;
        // The values `x += (dim - 1)` takes from 0 below `dim`, and the extrapolated index
        // of each.
        let ends = [0, dim - 1];
        let end = |x: usize| if x == 0 { 0 } else { dim + 1 };

        // Extrapolate faces.
        for idx in 0..dim {
            for jdx in 0..dim {
                for kdx in ends {
                    let index = end(kdx);
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    new_array.set_rgb(idx + 1, jdx + 1, index, extrapolate(rgb, center, scale));
                }
            }
        }
        for idx in 0..dim {
            for jdx in ends {
                for kdx in 0..dim {
                    let index = end(jdx);
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    new_array.set_rgb(idx + 1, index, kdx + 1, extrapolate(rgb, center, scale));
                }
            }
        }
        for idx in ends {
            for jdx in 0..dim {
                for kdx in 0..dim {
                    let index = end(idx);
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    new_array.set_rgb(index, jdx + 1, kdx + 1, extrapolate(rgb, center, scale));
                }
            }
        }

        // Extrapolate edges.
        for idx in ends {
            for jdx in ends {
                for kdx in 0..dim {
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    let rgb = extrapolate(rgb, center, scale);
                    new_array.set_rgb(end(idx), end(jdx), kdx + 1, rgb);
                }
            }
        }
        for idx in 0..dim {
            for jdx in ends {
                for kdx in ends {
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    let rgb = extrapolate(rgb, center, scale);
                    new_array.set_rgb(idx + 1, end(jdx), end(kdx), rgb);
                }
            }
        }
        for idx in ends {
            for jdx in 0..dim {
                for kdx in ends {
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    let rgb = extrapolate(rgb, center, scale);
                    new_array.set_rgb(end(idx), jdx + 1, end(kdx), rgb);
                }
            }
        }

        // Extrapolate corners.
        for idx in ends {
            for jdx in ends {
                for kdx in ends {
                    let rgb = array.get_rgb(idx, jdx, kdx);
                    let rgb = extrapolate(rgb, center, scale);
                    new_array.set_rgb(end(idx), end(jdx), end(kdx), rgb);
                }
            }
        }

        self.grvec = new_array.get_values().clone();
        Ok(())
    }

    /// Inverts packed RGBA pixels in place, alpha untouched: for each pixel, the first cube of
    /// the tree's depth-first walk that contains the clamped color, or 0 when none does.
    ///
    /// Upstream writes the output channels and copies alpha each time its walk climbs a level,
    /// from a result that changes only when the walk ends, and reads the color before the
    /// walk: the last write, after the walk, is the one that stays, so the port writes once,
    /// then. It never writes alpha (CLAUDE.md, "Channels that pass through").
    ///
    /// Port of `InvLut3DRenderer::apply` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:1595-1734
    /// @ v2.5.2).
    pub fn apply(&self, rgba: &mut [f32]) {
        let gsz = self.tree.get_grid_size();
        let max_dim = (gsz[0] - 3) as f32; // unextrapolated max
        let chans = self.tree.get_chans();
        let depth = self.tree.get_depth();
        let levels = self.tree.get_levels();
        let base_inds = self.tree.get_base_inds();

        let mut offs = [gsz[2] * gsz[1], gsz[2], 1];

        let list_len = 8;
        let ops_list: [i64; 8] = [0, 0, 1, 1, 1, 1, 1, 1];
        let entering_list: [usize; 8] = [2, 1, 0, 2, 0, 2, 0, 2];
        #[rustfmt::skip]
        let new_verts: [usize; 24] = [
            1, 0, 0,
            1, 1, 1,
            1, 1, 0,
            0, 1, 0,
            0, 1, 1,
            0, 0, 1,
            1, 0, 1,
            1, 0, 0 ];
        #[rustfmt::skip]
        let path_list: [usize; 24] = [
            0, 0, 0,
            0, 0, 0,
            0, 1, 2,
            1, 0, 2,
            1, 2, 0,
            2, 1, 0,
            2, 0, 1,
            0, 2, 1 ];
        let path_order: [usize; 3] = [1, 0, 2];
        let mut new_vert_list = [0usize; 8];
        for (i, v) in new_vert_list.iter_mut().enumerate() {
            // must happen before * chans
            *v = new_verts[i * 3] * offs[0]
                + new_verts[i * 3 + 1] * offs[1]
                + new_verts[i * 3 + 2] * offs[2];
        }
        for off in offs.iter_mut().take(chans) {
            *off *= chans;
        }

        const MAX_LEVELS: usize = 16;
        let mut current_child = [0usize; MAX_LEVELS];
        let mut current_num_children = [1usize; MAX_LEVELS];
        let mut current_child_ind = [0usize; MAX_LEVELS];

        for px in rgba.as_chunks_mut::<4>().0 {
            // Although the inverse LUT has been extrapolated, it may not be enough to cover an
            // HDR float image, so need to clamp.
            const IN_MAX: f32 = 1.0f32;
            let r = clamp(px[0], 0.0f32, IN_MAX);
            let g = clamp(px[1], 0.0f32, IN_MAX);
            let b = clamp(px[2], 0.0f32, IN_MAX);

            let depthm1 = depth as i64 - 1;
            let mut base_indx = [0usize; 3];

            current_num_children[0] = levels[0].child0_offsets.len();
            current_child[0] = 0;
            current_child_ind[0] = 0;

            // For now, if no result is found, return 0.
            let mut result = [0.0f32; 3];

            let mut level: i64 = 0;
            while level >= 0 {
                while current_child[level as usize] < current_num_children[level as usize] {
                    let lv = level as usize;
                    let node = current_child_ind[lv];
                    let tl = &levels[lv];
                    let in_range = r >= tl.min_vals[node * chans]
                        && g >= tl.min_vals[node * chans + 1]
                        && b >= tl.min_vals[node * chans + 2]
                        && r <= tl.max_vals[node * chans]
                        && g <= tl.max_vals[node * chans + 1]
                        && b <= tl.max_vals[node * chans + 2];
                    current_child[lv] += 1;
                    current_child_ind[lv] += 1;

                    if in_range {
                        if level == depthm1 {
                            base_indx[..chans].copy_from_slice(&base_inds[node].inds[..chans]);

                            let fxval = [r, g, b];

                            let valid = invert_hypercube(
                                3,
                                &mut result,
                                &self.grvec,
                                &offs,
                                &fxval,
                                &base_indx,
                                list_len,
                                &ops_list,
                                &entering_list,
                                &new_vert_list,
                                &path_list,
                                &path_order,
                            );

                            if valid {
                                level = 0; // to exit outer loop
                                break;
                            }
                        } else {
                            let new_level = lv + 1;
                            current_num_children[new_level] = tl.num_children[node];
                            current_child_ind[new_level] = tl.child0_offsets[node];
                            level = new_level as i64;
                            current_child[new_level] = 0;
                        }
                    }
                }
                level -= 1;
            }

            // Need to subtract 1 since the indices include the extrapolation.
            px[0] = clamp(result[0] - 1.0f32, 0.0f32, max_dim) * self.scale;
            px[1] = clamp(result[1] - 1.0f32, 0.0f32, max_dim) * self.scale;
            px[2] = clamp(result[2] - 1.0f32, 0.0f32, max_dim) * self.scale;
        }
    }
}

/// The inverse renderer processes `float` pixels in place ([`InvLut3DRenderer::apply`]); the
/// CPU engine uses it between F32 buffers only.
impl CpuOp for InvLut3DRenderer {
    fn apply(&self, rgba: &mut [f32]) {
        InvLut3DRenderer::apply(self, rgba);
    }
}

#[cfg(test)]
#[path = "inv_lut3d_tests.rs"]
mod tests;
