// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The no-ops: a port of the CPU side of `src/OpenColorIO/ops/noop/NoOps.h` and `NoOps.cpp` @
//! v2.5.2.
//!
//! Upstream has three no-op classes, private to NoOps.cpp: `AllocationNoOp`, `FileNoOp` and
//! `LookNoOp`. They leave pixels alone. The optimizer removes them all (`RemoveNoOpTypes`), and
//! the processor reads what they carry first: the files a transform loaded and the looks it
//! applied (`dumpMetadata`), and the GPU allocations of the legacy GPU path. Their data is a
//! `NoOpData` (a `FileNoOpData` for a `FileNoOp`); the op class holds the rest. The port
//! keeps the op class, and its state, in the data's [`NoOpKind`], since an [`Op`] is only its
//! data (`docs/architecture.md`, "Op data and ops").
//!
//! Not yet ported: the legacy GPU partition (`PartitionGPUOps`, `Create3DLut` and their
//! helpers), which needs the Lut3D op, and `dumpMetadata`, which needs the processor's
//! metadata (WP 1.8).

use std::mem::discriminant;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::format_metadata::FormatMetadataImpl;
use crate::op::{Op, OpVec};
use crate::op_data::OpData;
use crate::ops::allocation::AllocationData;

/// The data of a `FileNoOp`: the path of the file a `FileTransform` loads, and whether it is
/// loaded. It lets the `FileTransform` of a file that references itself, directly or not, be
/// detected.
///
/// Port of `FileNoOpData` (src/OpenColorIO/ops/noop/NoOps.h:53-74 @ v2.5.2).
#[derive(Debug)]
pub struct FileNoOpData {
    /// `m_path`.
    path: Vec<u8>,
    /// `m_complete`: `false` while the file is still being loaded. Upstream sets it through a
    /// pointer to const (a `mutable` member), so it is atomic here.
    complete: AtomicBool,
}

impl FileNoOpData {
    /// The data of the file at `path`, still being loaded.
    ///
    /// Port of `FileNoOpData::FileNoOpData(const std::string &)` (NoOps.h:61 @ v2.5.2).
    pub fn new(path: &[u8]) -> Self {
        FileNoOpData {
            path: path.to_vec(),
            complete: AtomicBool::new(false),
        }
    }

    /// Port of `FileNoOpData::getPath` (NoOps.h:63 @ v2.5.2).
    pub fn get_path(&self) -> &[u8] {
        &self.path
    }

    /// Records that the file is loaded, through a shared reference as upstream does through a
    /// pointer to const.
    ///
    /// Port of `FileNoOpData::setComplete` (NoOps.h:64 @ v2.5.2).
    pub fn set_complete(&self) {
        self.complete.store(true, Ordering::Relaxed);
    }

    /// Port of `FileNoOpData::getComplete` (NoOps.h:65 @ v2.5.2).
    pub fn get_complete(&self) -> bool {
        self.complete.load(Ordering::Relaxed)
    }
}

impl Clone for FileNoOpData {
    /// New data for the same path, still being loaded. Upstream deletes the copy constructor;
    /// its one copy is `FileNoOp::clone`, which makes new data for the path
    /// (src/OpenColorIO/ops/noop/NoOps.cpp:334-338 @ v2.5.2).
    fn clone(&self) -> Self {
        FileNoOpData::new(&self.path)
    }
}

/// Which no-op class holds the data, with that class's own state.
#[derive(Debug, Clone)]
pub enum NoOpKind {
    /// `AllocationNoOp`: the allocation of a color space, for the legacy GPU path (its
    /// `m_allocationData`).
    Allocation(AllocationData),
    /// `FileNoOp`: the file a `FileTransform` loads. Its data is a `FileNoOpData`.
    File(FileNoOpData),
    /// `LookNoOp`: the look a `LookTransform` applies (its `m_look`), `-` first when the
    /// look is inverted.
    Look(Vec<u8>),
}

/// The data of an op that leaves pixels alone.
///
/// Port of `NoOpData` (src/OpenColorIO/ops/noop/NoOps.h:39-51 @ v2.5.2), with the op class
/// that holds it and its state (see the module documentation).
#[derive(Debug, Clone)]
pub struct NoOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// The op class, and its state.
    kind: NoOpKind,
}

impl NoOpData {
    /// Empty metadata, and the class `kind`.
    ///
    /// Port of `NoOpData::NoOpData()` (NoOps.h:42 @ v2.5.2).
    pub fn new(kind: NoOpKind) -> Self {
        NoOpData {
            metadata: FormatMetadataImpl::default(),
            kind,
        }
    }

    /// The op class, and its state.
    pub fn kind(&self) -> &NoOpKind {
        &self.kind
    }

    /// The `FileNoOpData`, if this is the data of a `FileNoOp`: what upstream's
    /// `DynamicPtrCast<const FileNoOpData>` finds.
    pub fn file_data(&self) -> Option<&FileNoOpData> {
        match &self.kind {
            NoOpKind::File(file) => Some(file),
            NoOpKind::Allocation(_) | NoOpKind::Look(_) => None,
        }
    }

    /// The allocation of an `AllocationNoOp`: what upstream's `AllocationNoOp::getGpuAllocation`
    /// gives (src/OpenColorIO/ops/noop/NoOps.cpp:88-91 @ v2.5.2).
    pub fn get_gpu_allocation(&self) -> Option<&AllocationData> {
        match &self.kind {
            NoOpKind::Allocation(allocation) => Some(allocation),
            NoOpKind::File(_) | NoOpKind::Look(_) => None,
        }
    }

    /// Port of `NoOpData::validate` (NoOps.h:50 @ v2.5.2): nothing to check.
    pub fn validate(&self) {}

    /// Port of `NoOpData::isNoOp` (NoOps.h:46 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        true
    }

    /// Port of `NoOpData::isIdentity` (NoOps.h:47 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        true
    }

    /// Port of `NoOpData::hasChannelCrosstalk` (NoOps.h:48 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// The data's cache ID, which is empty. The ops' cache IDs are [`Op::get_cache_id`].
    ///
    /// Port of `NoOpData::getCacheID` (NoOps.h:49 @ v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    // The op classes' virtual methods, which `Op` calls for a no-op.

    /// A new op of the same class with the same state: new data, so empty metadata, and a
    /// `FileNoOpData` still being loaded.
    ///
    /// Port of `AllocationNoOp::clone`, `FileNoOp::clone` and `LookNoOp::clone`
    /// (src/OpenColorIO/ops/noop/NoOps.cpp:65-68, 334-338, 420-423 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::NoOp(NoOpData::new(self.kind.clone())))
    }

    /// Port of `AllocationNoOp::getInfo`, `FileNoOp::getInfo` and `LookNoOp::getInfo`
    /// (NoOps.cpp:40, 310, 396 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        match self.kind {
            NoOpKind::Allocation(_) => "<AllocationNoOp>",
            NoOpKind::File(_) => "<FileNoOp>",
            NoOpKind::Look(_) => "<LookNoOp>",
        }
    }

    /// Whether `op` is of the same class: what upstream's `DynamicPtrCast` to the class
    /// finds.
    ///
    /// Port of `AllocationNoOp::isSameType`, `FileNoOp::isSameType` and
    /// `LookNoOp::isSameType` (NoOps.cpp:70-75, 340-345, 425-430 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        match &**op.data() {
            OpData::NoOp(other) => discriminant(&self.kind) == discriminant(&other.kind),
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Reference(_) => false,
        }
    }

    /// Whether `op` undoes this op: whether it is of the same class.
    ///
    /// Port of `AllocationNoOp::isInverse`, `FileNoOp::isInverse` and `LookNoOp::isInverse`
    /// (NoOps.cpp:77-81, 347-350, 432-435 @ v2.5.2).
    pub(crate) fn is_inverse(&self, op: &Op) -> bool {
        self.is_same_type(op)
    }

    /// The op's cache ID: the allocation's for an `AllocationNoOp`, the look's name for a
    /// `LookNoOp`, and nothing for a `FileNoOp`.
    ///
    /// `FileNoOp::getCacheID` returns its `m_fileReference`, which its constructor never sets:
    /// it gives the path to its `FileNoOpData` instead. So the cache ID is empty
    /// (improvement candidate I-40). A processor's cache ID skips no-ops, but
    /// [`serialize_op_vec`](crate::op::serialize_op_vec) prints it.
    ///
    /// Port of `AllocationNoOp::getCacheID`, `FileNoOp::getCacheID` and `LookNoOp::getCacheID`
    /// (NoOps.cpp:83-86, 300-304, 358-361, 442-445 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        match &self.kind {
            NoOpKind::Allocation(allocation) => allocation.get_cache_id().into_bytes(),
            NoOpKind::File(_) => Vec::new(),
            NoOpKind::Look(look) => look.clone(),
        }
    }
}

/// Appends an `AllocationNoOp` that carries `allocation_data`.
///
/// Port of `CreateGpuAllocationNoOp` (src/OpenColorIO/ops/noop/NoOps.cpp:104-107 @ v2.5.2),
/// which `ops/allocation/AllocationOp.h` declares.
pub fn create_gpu_allocation_no_op(ops: &mut OpVec, allocation_data: &AllocationData) {
    let data = NoOpData::new(NoOpKind::Allocation(allocation_data.clone()));
    ops.push_back(Op::new(OpData::NoOp(data)));
}

/// Appends a `FileNoOp` for the file at `file_reference`, still being loaded.
///
/// Port of `CreateFileNoOp` (src/OpenColorIO/ops/noop/NoOps.cpp:365-369 @ v2.5.2).
pub fn create_file_no_op(ops: &mut OpVec, file_reference: &[u8]) {
    let data = NoOpData::new(NoOpKind::File(FileNoOpData::new(file_reference)));
    ops.push_back(Op::new(OpData::NoOp(data)));
}

/// Appends a `LookNoOp` for the look `look`.
///
/// Port of `CreateLookNoOp` (src/OpenColorIO/ops/noop/NoOps.cpp:449-453 @ v2.5.2).
pub fn create_look_no_op(ops: &mut OpVec, look: &[u8]) {
    let data = NoOpData::new(NoOpKind::Look(look.to_vec()));
    ops.push_back(Op::new(OpData::NoOp(data)));
}

#[cfg(test)]
#[path = "no_ops_tests.rs"]
mod tests;
