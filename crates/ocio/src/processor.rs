// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Processors: a port of `src/OpenColorIO/Processor.h` and `Processor.cpp` @ v2.5.2, with the
//! CPU and GPU processors it makes. The legacy GPU processor comes with the 3D LUT op (its
//! `Create3DLut`), `setColorSpaceConversion` and `concatenate` with the config (Phase 3).
//!
//! A processor owns its ops and finalizes them once, in [`Processor::set_transform`]. The
//! processors it makes ([`Processor::optimized_processor_with_bit_depths`], the CPU
//! processors) copy the list: the copies share the ops' data, and an op that changes its data
//! copies it first (`Arc::make_mut`), so the processor's own ops never change. Upstream's copies
//! share the `Op` objects themselves, and re-finalize them, which leaves finalized ops as they
//! are.

use std::collections::BTreeSet;
use std::ffi::c_ulong;
use std::sync::{Arc, Mutex};

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    BitDepth, DynamicPropertyType, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::noop::NoOpKind;
use ocio_ops::platform::getenv;

use crate::caching::{ProcessorCache, std_hash_string};
use crate::config::Config;
use crate::context::Context;
use crate::transform::{Transform, build_ops, create_transform};
use crate::transforms::group_transform::GroupTransform;

/// `OCIO_OPTIMIZATION_FLAGS`: overrides the optimization flags of every optimized processor.
///
/// Port of `OCIO_OPTIMIZATION_FLAGS_ENVVAR` (include/OpenColorIO/OpenColorTypes.h:832 @
/// v2.5.2).
pub const OCIO_OPTIMIZATION_FLAGS_ENVVAR: &str = "OCIO_OPTIMIZATION_FLAGS";

/// How a config and its processors cache the processors they make: bit flags.
///
/// Port of `ProcessorCacheFlags` (include/OpenColorIO/OpenColorTypes.h:734-743 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessorCacheFlags(pub u32);

impl ProcessorCacheFlags {
    /// `PROCESSOR_CACHE_OFF`.
    pub const OFF: ProcessorCacheFlags = ProcessorCacheFlags(0x00);
    /// `PROCESSOR_CACHE_ENABLED`: enable the cache.
    pub const ENABLED: ProcessorCacheFlags = ProcessorCacheFlags(0x01);
    /// `PROCESSOR_CACHE_SHARE_DYN_PROPERTIES`: share cached processors even when they have
    /// dynamic properties.
    pub const SHARE_DYN_PROPERTIES: ProcessorCacheFlags = ProcessorCacheFlags(0x02);
    /// `PROCESSOR_CACHE_DEFAULT`: both.
    pub const DEFAULT: ProcessorCacheFlags = ProcessorCacheFlags(0x03);

    /// `(flags & query) == query`.
    pub fn has_flag(self, query: ProcessorCacheFlags) -> bool {
        (self.0 & query.0) == query.0
    }
}

/// The bytes of a C string: up to the first NUL.
fn c_string(bytes: &[u8]) -> &[u8] {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..len]
}

/// What a processor read: the files and the looks.
///
/// Port of `ProcessorMetadata` (src/OpenColorIO/Processor.cpp:24-97 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessorMetadata {
    /// `files`: a `std::set<std::string>`, in byte order.
    files: BTreeSet<Vec<u8>>,
    /// `looks`.
    looks: Vec<Vec<u8>>,
}

impl ProcessorMetadata {
    /// Port of `ProcessorMetadata::Create` (Processor.cpp:34-37 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ProcessorMetadata {
        ProcessorMetadata::default()
    }

    /// Port of `ProcessorMetadata::getNumFiles` (Processor.cpp:54-57 @ v2.5.2).
    #[doc(alias = "getNumFiles")]
    pub fn num_files(&self) -> i32 {
        self.files.len() as i32
    }

    /// File `index`, in byte order; empty outside the files.
    ///
    /// Port of `ProcessorMetadata::getFile` (Processor.cpp:59-71 @ v2.5.2).
    #[doc(alias = "getFile")]
    pub fn file(&self, index: i32) -> &[u8] {
        if index < 0 || index >= self.files.len() as i32 {
            return b"";
        }
        self.files
            .iter()
            .nth(index as usize)
            .expect("an index inside the files")
    }

    /// `fname` is a C string: it ends at its first NUL.
    ///
    /// Port of `ProcessorMetadata::addFile` (Processor.cpp:73-76 @ v2.5.2).
    #[doc(alias = "addFile")]
    pub fn add_file(&mut self, fname: &[u8]) {
        self.files.insert(c_string(fname).to_vec());
    }

    /// Port of `ProcessorMetadata::getNumLooks` (Processor.cpp:78-81 @ v2.5.2).
    #[doc(alias = "getNumLooks")]
    pub fn num_looks(&self) -> i32 {
        self.looks.len() as i32
    }

    /// Look `index`; empty outside the looks.
    ///
    /// Port of `ProcessorMetadata::getLook` (Processor.cpp:83-92 @ v2.5.2).
    #[doc(alias = "getLook")]
    pub fn look(&self, index: i32) -> &[u8] {
        if index < 0 || index >= self.looks.len() as i32 {
            return b"";
        }
        &self.looks[index as usize]
    }

    /// `look` is a C string: it ends at its first NUL.
    ///
    /// Port of `ProcessorMetadata::addLook` (Processor.cpp:94-97 @ v2.5.2).
    #[doc(alias = "addLook")]
    pub fn add_look(&mut self, look: &[u8]) {
        self.looks.push(c_string(look).to_vec());
    }
}

/// A processor: the finalized ops of a transform, from which it makes optimized processors and
/// CPU processors, each cached by its arguments.
///
/// Port of `Processor` and `Processor::Impl` (include/OpenColorIO/OpenColorIO.h,
/// src/OpenColorIO/Processor.h, Processor.cpp:101-664 @ v2.5.2).
#[derive(Debug)]
pub struct Processor {
    /// `m_metadata`, shared with the processors made from this one, as upstream does.
    metadata: Arc<ProcessorMetadata>,
    /// `m_ops`.
    ops: OpVec,
    /// `m_cacheID`, once computed (under `m_resultsCacheMutex`).
    cache_id: Mutex<Option<String>>,
    /// `m_cacheFlags`.
    cache_flags: ProcessorCacheFlags,
    /// `m_optProcessorCache`.
    opt_processor_cache: ProcessorCache<u64, Arc<Processor>>,
    /// `m_gpuProcessorCache`, by the flags themselves.
    gpu_processor_cache: ProcessorCache<u64, Arc<GpuProcessor>>,
    /// `m_cpuProcessorCache`.
    cpu_processor_cache: ProcessorCache<u64, Arc<CpuProcessor>>,
}

/// `oss << inBitDepth << outBitDepth << oFlags`, the key text of the processors' caches: the
/// enumerators' values and the flags as decimal integers.
fn depths_and_flags_key(in_bd: BitDepth, out_bd: BitDepth, flags: OptimizationFlags) -> u64 {
    let text = format!("{}{}{}", in_bd as i32, out_bd as i32, flags.0);
    std_hash_string(text.as_bytes())
}

/// The GPU processors' key: the flags' value, an `unsigned long` (32 bits on Windows, 64 on
/// Linux) widened to `std::size_t`.
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::useless_conversion)]
fn flags_key(flags: OptimizationFlags) -> u64 {
    u64::from(flags.0)
}

/// `std::stoul(text, nullptr, 0)`, with each wheel's C++ library's message (`e.what()`) for
/// text it can't convert or a value out of range: "invalid stoul argument" and "stoul argument
/// out of range" with MSVC (Windows), "stoul" with libstdc++ (Linux). Both call `strtoul` with
/// the base 0, whose `unsigned long` is 32 bits on Windows and 64 on Linux.
fn stoul(text: &[u8]) -> std::result::Result<c_ulong, &'static str> {
    let (value, end, erange) = strtoul_base0(text);
    if end == 0 {
        return Err(if cfg!(windows) {
            "invalid stoul argument"
        } else {
            "stoul"
        });
    }
    if erange {
        return Err(if cfg!(windows) {
            "stoul argument out of range"
        } else {
            "stoul"
        });
    }
    Ok(value)
}

/// `strtoul(text, &end, 0)` in the "C" locale: the value, the offset of `end` (0 when nothing
/// converts) and whether `errno` is `ERANGE`. Leading white space, a sign (a minus negates the
/// value modulo 2^N), then `0x`/`0X` and hexadecimal digits, `0` and octal digits, or decimal
/// digits; an overflow gives `ULONG_MAX`. A `0x` without a hexadecimal digit after it converts
/// nothing with the Windows C runtime, and converts its `0` with glibc. Checked against the C
/// runtime (`processor_tests.rs`).
///
/// The wheels run under Python, whose locale isn't "C", but no byte that a locale could add to
/// the white space reaches `strtoul` first: OCIO reads the variable as UTF-8 on Windows, where
/// such characters start with a lead byte, and glibc's UTF-8 locales add none.
fn strtoul_base0(text: &[u8]) -> (c_ulong, usize, bool) {
    let len = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    let s = &text[..len];
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let mut negative = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        negative = s[i] == b'-';
        i += 1;
    }
    let digit = |b: u8, base: u32| (b as char).to_digit(base);
    let hex_prefix = i + 1 < s.len() && s[i] == b'0' && (s[i + 1] == b'x' || s[i + 1] == b'X');
    let hex_digit_follows = i + 2 < s.len() && digit(s[i + 2], 16).is_some();
    let base = if hex_prefix && (hex_digit_follows || cfg!(windows)) {
        i += 2;
        16
    } else if i < s.len() && s[i] == b'0' {
        8
    } else {
        10
    };
    let start = i;
    let mut value: c_ulong = 0;
    let mut overflow = false;
    while i < s.len() {
        let Some(d) = digit(s[i], base) else {
            break;
        };
        match value
            .checked_mul(base as c_ulong)
            .and_then(|v| v.checked_add(d as c_ulong))
        {
            Some(v) => value = v,
            None => overflow = true,
        }
        i += 1;
    }
    if i == start {
        return (0, 0, false);
    }
    if overflow {
        return (c_ulong::MAX, i, true);
    }
    if negative {
        value = value.wrapping_neg();
    }
    (value, i, false)
}

/// The flags of `OCIO_OPTIMIZATION_FLAGS` when it is set and not empty, `o_flags` otherwise:
/// "Illegal value for OCIO_OPTIMIZATION_FLAGS: <what>" for a value `std::stoul` refuses.
///
/// Port of `EnvironmentOverride` (src/OpenColorIO/Processor.cpp:354-374 @ v2.5.2).
fn environment_override(o_flags: OptimizationFlags) -> Result<OptimizationFlags> {
    let env_flag = getenv(OCIO_OPTIMIZATION_FLAGS_ENVVAR).unwrap_or_default();
    if !env_flag.is_empty() {
        // Use 0 to allow base to be determined by the format.
        return match stoul(env_flag.as_bytes()) {
            Ok(value) => Ok(OptimizationFlags(value)),
            Err(what) => Err(Exception::new(format!(
                "Illegal value for {OCIO_OPTIMIZATION_FLAGS_ENVVAR}: {what}"
            ))),
        };
    }
    Ok(o_flags)
}

impl Processor {
    /// An empty processor, with the default cache flags.
    ///
    /// Port of `Processor::Create` and `Processor::Impl::Impl` (Processor.cpp:101-104,
    /// 228-231 @ v2.5.2).
    pub(crate) fn new() -> Processor {
        Processor {
            metadata: Arc::new(ProcessorMetadata::new()),
            ops: OpVec::new(),
            cache_id: Mutex::new(None),
            cache_flags: ProcessorCacheFlags::DEFAULT,
            opt_processor_cache: ProcessorCache::new(),
            gpu_processor_cache: ProcessorCache::new(),
            cpu_processor_cache: ProcessorCache::new(),
        }
    }

    /// A copy of `self` without its cache ID and with empty caches, enabled by its flags.
    ///
    /// Port of `Processor::Impl::operator=` (Processor.cpp:237-263 @ v2.5.2) on a new
    /// processor.
    fn copy(&self) -> Processor {
        let mut proc = Processor::new();
        proc.metadata = self.metadata.clone();
        proc.ops = self.ops.clone();
        proc.set_processor_cache_flags(self.cache_flags);
        proc
    }

    /// Whether the processor's ops leave every pixel as it is.
    ///
    /// Port of `Processor::Impl::isNoOp` (Processor.cpp:265-268 @ v2.5.2).
    #[doc(alias = "isNoOp")]
    pub fn is_no_op(&self) -> Result<bool> {
        self.ops.is_no_op()
    }

    /// Port of `Processor::Impl::hasChannelCrosstalk` (Processor.cpp:270-273 @ v2.5.2).
    #[doc(alias = "hasChannelCrosstalk")]
    pub fn has_channel_crosstalk(&self) -> bool {
        self.ops.has_channel_crosstalk()
    }

    /// Port of `Processor::Impl::getProcessorMetadata` (Processor.cpp:275-278 @ v2.5.2).
    #[doc(alias = "getProcessorMetadata")]
    pub fn processor_metadata(&self) -> &ProcessorMetadata {
        &self.metadata
    }

    /// The metadata of the processor's ops (the first group's).
    ///
    /// Port of `Processor::Impl::getFormatMetadata` (Processor.cpp:280-283 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.ops.get_format_metadata()
    }

    /// The number of ops.
    ///
    /// Port of `Processor::Impl::getNumTransforms` (Processor.cpp:285-288 @ v2.5.2).
    #[doc(alias = "getNumTransforms")]
    pub fn num_transforms(&self) -> i32 {
        self.ops.len() as i32
    }

    /// Op `index`'s metadata: "Processor::getTransformFormatMetadata: index out of range."
    /// outside the ops.
    ///
    /// Port of `Processor::Impl::getTransformFormatMetadata` (Processor.cpp:290-298 @
    /// v2.5.2).
    #[doc(alias = "getTransformFormatMetadata")]
    pub fn transform_format_metadata(&self, index: i32) -> Result<&FormatMetadataImpl> {
        if index < 0 || index >= self.ops.len() as i32 {
            return Err(Exception::new(
                "Processor::getTransformFormatMetadata: index out of range.",
            ));
        }
        Ok(self.ops[index as usize].data().get_format_metadata())
    }

    /// A group of the transforms the ops render, with the processor's metadata.
    ///
    /// Port of `Processor::Impl::createGroupTransform` (Processor.cpp:300-314 @ v2.5.2).
    #[doc(alias = "createGroupTransform")]
    pub fn create_group_transform(&self) -> Result<GroupTransform> {
        let mut group = GroupTransform::new();

        // Copy format metadata.
        *group.format_metadata_mut() = self.format_metadata().clone();

        // Build transforms from ops.
        for op in self.ops.iter() {
            create_transform(&mut group, op)?;
        }

        Ok(group)
    }

    /// Port of `Processor::Impl::isDynamic` (Processor.cpp:316-319 @ v2.5.2).
    #[doc(alias = "isDynamic")]
    pub fn is_dynamic(&self) -> bool {
        self.ops.is_dynamic()
    }

    /// Port of `Processor::Impl::hasDynamicProperty` (Processor.cpp:321-324 @ v2.5.2).
    #[doc(alias = "hasDynamicProperty")]
    pub fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        self.ops.has_dynamic_property(type_)
    }

    /// Port of `Processor::Impl::getDynamicProperty` (Processor.cpp:326-329 @ v2.5.2).
    #[doc(alias = "getDynamicProperty")]
    pub fn dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        self.ops.get_dynamic_property(type_)
    }

    /// `<NOOP>` without ops, else the hash of the ops' cache IDs; computed once.
    ///
    /// Port of `Processor::Impl::getCacheID` (Processor.cpp:331-348 @ v2.5.2).
    #[doc(alias = "getCacheID")]
    pub fn cache_id(&self) -> Result<String> {
        let mut cache_id = self.cache_id.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = &*cache_id {
            return Ok(id.clone());
        }

        let id = if self.ops.is_empty() {
            "<NOOP>".to_string()
        } else {
            let fullstr = self.ops.get_cache_id()?;
            cache_id_hash(&fullstr)
        };
        *cache_id = Some(id.clone());
        Ok(id)
    }

    /// [`optimized_processor_with_bit_depths`](Self::optimized_processor_with_bit_depths)
    /// for F32 in and out.
    ///
    /// Port of `Processor::Impl::getOptimizedProcessor(OptimizationFlags)`
    /// (Processor.cpp:377-380 @ v2.5.2).
    #[doc(alias = "getOptimizedProcessor")]
    pub fn optimized_processor(&self, o_flags: OptimizationFlags) -> Result<Arc<Processor>> {
        self.optimized_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, o_flags)
    }

    /// A processor of these ops optimized for the bit depths and the flags (which
    /// `OCIO_OPTIMIZATION_FLAGS` overrides), cached under them.
    ///
    /// Port of `Processor::Impl::getOptimizedProcessor(BitDepth, BitDepth, OptimizationFlags)`
    /// (Processor.cpp:382-433 @ v2.5.2).
    #[doc(alias = "getOptimizedProcessor")]
    pub fn optimized_processor_with_bit_depths(
        &self,
        in_bit_depth: BitDepth,
        out_bit_depth: BitDepth,
        o_flags: OptimizationFlags,
    ) -> Result<Arc<Processor>> {
        let create_processor = |o_flags: OptimizationFlags| -> Result<Arc<Processor>> {
            let mut proc = self.copy();
            proc.ops.finalize()?;
            proc.ops.optimize(o_flags)?;
            proc.ops
                .optimize_for_bitdepth(in_bit_depth, out_bit_depth, o_flags)?;
            proc.ops.validate_dynamic_properties()?;
            Ok(Arc::new(proc))
        };

        let o_flags = environment_override(o_flags)?;

        if let Some(mut cache) = self.opt_processor_cache.lock() {
            let key = depths_and_flags_key(in_bit_depth, out_bit_depth, o_flags);
            if let Some(processor) = cache.entries().get(&key) {
                return Ok(processor.clone());
            }
            // Note: Some combinations of bit-depth and opt flags will produce identical
            // Processors. Duplicates could be identified by computing the Processor cacheID,
            // but that is too slow to attempt here.
            let processor = create_processor(o_flags)?;
            cache.entries().insert(key, processor.clone());
            return Ok(processor);
        }
        create_processor(o_flags)
    }

    /// The GPU processor of the default optimization.
    ///
    /// Port of `Processor::Impl::getDefaultGPUProcessor` (Processor.cpp:437-440 @ v2.5.2).
    #[doc(alias = "getDefaultGPUProcessor")]
    pub fn default_gpu_processor(&self) -> Result<Arc<GpuProcessor>> {
        self.gpu_processor(&self.ops, OptimizationFlags::DEFAULT)
    }

    /// The GPU processor of these flags.
    ///
    /// Port of `Processor::Impl::getOptimizedGPUProcessor` (Processor.cpp:442-445 @ v2.5.2).
    #[doc(alias = "getOptimizedGPUProcessor")]
    pub fn optimized_gpu_processor(&self, o_flags: OptimizationFlags) -> Result<Arc<GpuProcessor>> {
        self.gpu_processor(&self.ops, o_flags)
    }

    /// The GPU processor of `gpu_ops` for the flags (which `OCIO_OPTIMIZATION_FLAGS`
    /// overrides), cached under the flags alone: unlike the CPU processors, whatever the ops'
    /// dynamic properties and the cache flags' `SHARE_DYN_PROPERTIES`.
    ///
    /// Port of `Processor::Impl::getGPUProcessor` (Processor.cpp:491-523 @ v2.5.2).
    fn gpu_processor(
        &self,
        gpu_ops: &OpVec,
        o_flags: OptimizationFlags,
    ) -> Result<Arc<GpuProcessor>> {
        // Helper method.
        let create_processor = |o_flags: OptimizationFlags| -> Result<Arc<GpuProcessor>> {
            Ok(Arc::new(GpuProcessor::new(gpu_ops, o_flags)?))
        };

        let o_flags = environment_override(o_flags)?;

        if let Some(mut cache) = self.gpu_processor_cache.lock() {
            let key = flags_key(o_flags);
            if let Some(processor) = cache.entries().get(&key) {
                return Ok(processor.clone());
            }
            let processor = create_processor(o_flags)?;
            cache.entries().insert(key, processor.clone());
            return Ok(processor);
        }
        create_processor(o_flags)
    }

    /// The CPU processor of the default optimization, F32 in and out.
    ///
    /// Port of `Processor::Impl::getDefaultCPUProcessor` (Processor.cpp:527-530 @ v2.5.2).
    #[doc(alias = "getDefaultCPUProcessor")]
    pub fn default_cpu_processor(&self) -> Result<Arc<CpuProcessor>> {
        self.optimized_cpu_processor(OptimizationFlags::DEFAULT)
    }

    /// The CPU processor of these flags, F32 in and out.
    ///
    /// Port of `Processor::Impl::getOptimizedCPUProcessor(OptimizationFlags)`
    /// (Processor.cpp:532-535 @ v2.5.2).
    #[doc(alias = "getOptimizedCPUProcessor")]
    pub fn optimized_cpu_processor(&self, o_flags: OptimizationFlags) -> Result<Arc<CpuProcessor>> {
        self.optimized_cpu_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, o_flags)
    }

    /// The CPU processor of the ops for the bit depths and the flags (which
    /// `OCIO_OPTIMIZATION_FLAGS` overrides), cached under them unless the ops are dynamic and
    /// the flags don't share dynamic properties.
    ///
    /// Port of `Processor::Impl::getOptimizedCPUProcessor(BitDepth, BitDepth,
    /// OptimizationFlags)` (Processor.cpp:537-582 @ v2.5.2).
    #[doc(alias = "getOptimizedCPUProcessor")]
    pub fn optimized_cpu_processor_with_bit_depths(
        &self,
        in_bit_depth: BitDepth,
        out_bit_depth: BitDepth,
        o_flags: OptimizationFlags,
    ) -> Result<Arc<CpuProcessor>> {
        let create_processor = |o_flags: OptimizationFlags| -> Result<Arc<CpuProcessor>> {
            Ok(Arc::new(CpuProcessor::new(
                &self.ops,
                in_bit_depth,
                out_bit_depth,
                o_flags,
            )?))
        };

        let o_flags = environment_override(o_flags)?;

        let share_dynamic_properties = self
            .cache_flags
            .has_flag(ProcessorCacheFlags::SHARE_DYN_PROPERTIES);

        let use_cache = if self.ops.is_dynamic() {
            share_dynamic_properties
        } else {
            true
        };

        if use_cache && let Some(mut cache) = self.cpu_processor_cache.lock() {
            let key = depths_and_flags_key(in_bit_depth, out_bit_depth, o_flags);
            if let Some(processor) = cache.entries().get(&key) {
                return Ok(processor.clone());
            }
            let processor = create_processor(o_flags)?;
            cache.entries().insert(key, processor.clone());
            return Ok(processor);
        }
        create_processor(o_flags)
    }

    /// Sets the cache flags, and enables the caches by them.
    ///
    /// Port of `Processor::Impl::setProcessorCacheFlags` (Processor.cpp:584-593 @ v2.5.2).
    pub(crate) fn set_processor_cache_flags(&mut self, flags: ProcessorCacheFlags) {
        self.cache_flags = flags;

        let cache_enabled = self.cache_flags.has_flag(ProcessorCacheFlags::ENABLED);

        self.opt_processor_cache.enable(cache_enabled);
        self.gpu_processor_cache.enable(cache_enabled);
        self.cpu_processor_cache.enable(cache_enabled);
    }

    /// Builds the ops of `transform` in the direction `direction`, after validating it, and
    /// finalizes them (the no-ops stay, for the legacy GPU processor).
    ///
    /// Port of `Processor::Impl::setTransform` (Processor.cpp:623-641 @ v2.5.2).
    pub(crate) fn set_transform(
        &mut self,
        config: &Config,
        context: &Context,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<()> {
        if !self.ops.is_empty() {
            return Err(Exception::new("Internal error: Processor should be empty"));
        }

        transform.validate()?;

        build_ops(&mut self.ops, config, context, transform, direction)?;

        // NB: No-ops are not removed yet since they are still needed to build the legacy GPU
        // processor.
        self.ops.finalize()?;

        self.ops.validate_dynamic_properties()
    }

    /// Collects the files and looks the ops read, before the no-ops are removed: each op's
    /// `dumpMetadata` (a `FileNoOp`'s file, a `LookNoOp`'s look; nothing for the others).
    ///
    /// Port of `Processor::Impl::computeMetadata` (Processor.cpp:655-664 @ v2.5.2), with
    /// `Op::dumpMetadata` and its overrides (src/OpenColorIO/Op.h:221-222,
    /// ops/noop/NoOps.cpp:352-356, 438-441 @ v2.5.2).
    pub(crate) fn compute_metadata(&mut self) {
        let metadata = Arc::make_mut(&mut self.metadata);
        for op in self.ops.iter() {
            if let OpData::NoOp(data) = &**op.data() {
                match data.kind() {
                    NoOpKind::File(file) => metadata.add_file(file.get_path()),
                    NoOpKind::Look(look) => metadata.add_look(look),
                    NoOpKind::Allocation(_) => {}
                }
            }
        }
    }

    /// The processor's ops, for the port's tests.
    #[doc(hidden)]
    pub fn ops(&self) -> &OpVec {
        &self.ops
    }
}

#[cfg(test)]
#[path = "processor_tests.rs"]
mod tests;
