// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exact inverse of a 3D LUT: a port of `InvLut3DRenderer`'s `RangeTree`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU.cpp:78-160 and 1080-1388 @ v2.5.2).
//!
//! **Integer widths.** Upstream counts and indexes in `unsigned long` (32 bits on Windows, 64
//! on Linux). Here they are `usize`. The renderer builds the tree on the LUT extrapolated by
//! one entry per side, so the grid has at most 131 entries per side and the tree at most 8
//! levels: every count, offset and hash then stays below 2^31, and both widths give the same
//! values.
//!
//! **Arithmetic.** The tree's ranges are `float`, compared with C++'s `std::min` and
//! `std::max` ([`std_min`], [`std_max`]).
//!
//! Not ported yet: the renderer itself (WP 2.2d2).

use crate::math_utils::{std_max, std_min};

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

#[cfg(test)]
#[path = "inv_lut3d_tests.rs"]
mod tests;
