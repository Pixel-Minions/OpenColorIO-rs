// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! What expat's allocation tracker counts: the blocks `xmlparse.c` allocates with `MALLOC` and
//! `REALLOC`, block for block, so that the tracker (`MALLOC_TRACKER`, new in 2.7.2) refuses
//! the same allocation as in the wheels.
//!
//! The port keeps its strings and tables in its own structures; this module keeps, beside
//! them, the sizes expat would have allocated for its: the string pools' blocks
//! (`STRING_POOL`, `poolGrow`), the hash tables' arrays and entries (`lookup`), the open tags'
//! name buffers, the attribute array, the open entities, the elements' default attributes and
//! the group connector. The sizes are those of the C structures as the wheels' compilers lay
//! them out (x86-64: 8-byte pointers and `size_t`, 4-byte `int`; `long` is 4 bytes on Windows
//! and 8 on Linux, which only the parser's own structure holds); they were checked with
//! `sizeof` and `offsetof` on the expat 2.7.2 sources, built as the wheels build them, with
//! MinGW-w64 GCC (Windows' LLP64 layout, as MSVC's) and with GCC on Rocky Linux 9.

use super::xmlparse::XmlError;

/// `sizeof(size_t)`: the size `expat_malloc` records before each block.
pub(crate) const SIZE_T: u64 = 8;

/// `sizeof(struct XML_ParserStruct)` (xmlparse.c:663-785) as the wheel's compiler lays it out:
/// it holds `unsigned long` and `long` fields (`m_hash_secret_salt`, `XML_Index`, `XML_Size`,
/// the accounting's and the tracker's debug levels), 4 bytes with MSVC and 8 with GCC.
#[cfg(windows)]
pub(crate) const SIZEOF_PARSER: u64 = 1048;
/// `sizeof(struct XML_ParserStruct)` (xmlparse.c:663-785) with GCC on x86-64 Linux.
#[cfg(not(windows))]
pub(crate) const SIZEOF_PARSER: u64 = 1088;
/// `sizeof(DTD)` (xmlparse.c:393-420).
pub(crate) const SIZEOF_DTD: usize = 360;
/// `sizeof(TAG)` (xmlparse.c:308-316).
pub(crate) const SIZEOF_TAG: usize = 88;
/// `sizeof(ENTITY)` (xmlparse.c:318-334).
pub(crate) const SIZEOF_ENTITY: usize = 64;
/// `sizeof(ELEMENT_TYPE)` (xmlparse.c:384-391).
pub(crate) const SIZEOF_ELEMENT_TYPE: usize = 40;
/// `sizeof(ATTRIBUTE_ID)` (xmlparse.c:365-370).
pub(crate) const SIZEOF_ATTRIBUTE_ID: usize = 24;
/// `sizeof(PREFIX)` (xmlparse.c:281-284).
pub(crate) const SIZEOF_PREFIX: usize = 16;
/// `sizeof(DEFAULT_ATTRIBUTE)` (xmlparse.c:372-376).
pub(crate) const SIZEOF_DEFAULT_ATTRIBUTE: usize = 24;
/// `sizeof(OPEN_INTERNAL_ENTITY)` (xmlparse.c:428-436).
pub(crate) const SIZEOF_OPEN_INTERNAL_ENTITY: usize = 48;
/// `sizeof(ATTRIBUTE)` (xmltok.h).
pub(crate) const SIZEOF_ATTRIBUTE: usize = 32;
/// `sizeof(NAMED *)`: a hash table's slot.
const SIZEOF_NAMED_PTR: usize = 8;
/// `offsetof(BLOCK, s)` (xmlparse.c:348-352): a pool block's header.
const OFFSETOF_BLOCK_S: usize = 12;
/// `INIT_TAG_BUF_SIZE` (xmlparse.c:262).
const INIT_TAG_BUF_SIZE: usize = 32;
/// `INIT_BLOCK_SIZE` (xmlparse.c:266).
const INIT_BLOCK_SIZE: i32 = 1024;
/// `INIT_POWER` (xmlparse.c:7776): a hash table's first size is `1 << INIT_POWER`.
const INIT_POWER: u8 = 6;

/// A block `expat_malloc` counted: the size it recorded in the `size_t` before the block.
#[derive(Debug)]
pub(crate) struct Block {
    pub(crate) size: usize,
}

/// The allocation tracker, and the direct bytes its amplification is computed from.
///
/// Port of `MALLOC_TRACKER` (xmlparse.c:455-461), without the debug fields, and of
/// `expat_heap_increase_tolerable`, `expat_malloc`, `expat_free` and `expat_realloc`
/// (xmlparse.c:812-1005), without the allocations themselves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MallocTracker {
    /// `bytesAllocated`.
    pub(crate) bytes_allocated: u64,
    /// `maximumAmplificationFactor`.
    pub(crate) maximum_amplification_factor: f32,
    /// `activationThresholdBytes`.
    pub(crate) activation_threshold_bytes: u64,
    /// `m_accounting.countBytesDirect`, as the parser's accounting last set it.
    pub(crate) count_bytes_direct: u64,
}

impl MallocTracker {
    /// Whether the tracker allows `increase` more bytes.
    ///
    /// Port of `expat_heap_increase_tolerable` (xmlparse.c:812-844), without the report.
    pub(crate) fn increase_tolerable(&self, increase: u64) -> bool {
        // Detect integer overflow
        if u64::MAX - self.bytes_allocated < increase {
            return false;
        }
        let new_total = self.bytes_allocated + increase;
        if new_total >= self.activation_threshold_bytes {
            // NOTE: This can be +infinity when dividing by zero but not -nan
            let amplification = new_total as f32 / self.count_bytes_direct as f32;
            if amplification > self.maximum_amplification_factor {
                return false;
            }
        }
        true
    }

    /// Counts a block of `size` bytes, or refuses it (`NULL`, out of memory).
    ///
    /// Port of `expat_malloc` (xmlparse.c:846-898).
    pub(crate) fn malloc(&mut self, size: usize) -> Result<Block, XmlError> {
        // Detect integer overflow
        if usize::MAX - size < SIZE_T as usize {
            return Err(XmlError::NoMemory);
        }
        let bytes_to_allocate = SIZE_T + size as u64;
        if u64::MAX - self.bytes_allocated < bytes_to_allocate {
            return Err(XmlError::NoMemory); // i.e. signal integer overflow as out-of-memory
        }
        if !self.increase_tolerable(bytes_to_allocate) {
            return Err(XmlError::NoMemory); // i.e. signal violation as out-of-memory
        }
        // Update accounting
        self.bytes_allocated += bytes_to_allocate;
        Ok(Block { size })
    }

    /// Port of `expat_free` (xmlparse.c:900-933).
    pub(crate) fn free(&mut self, block: Block) {
        self.bytes_allocated -= SIZE_T + block.size as u64;
    }

    /// Resizes a counted block to `size` bytes (not 0); refused, the block unchanged, when the
    /// tracker refuses the increase.
    ///
    /// Port of `expat_realloc` (xmlparse.c:935-1005) for a block (`ptr` not null).
    pub(crate) fn realloc(&mut self, block: &mut Block, size: usize) -> Result<(), XmlError> {
        let prev_size = block.size;
        // Classify upcoming change
        let is_increase = size > prev_size;
        let abs_diff = size.abs_diff(prev_size) as u64;
        // Ask for permission from accounting
        if is_increase && !self.increase_tolerable(abs_diff) {
            return Err(XmlError::NoMemory); // i.e. signal violation as out-of-memory
        }
        if is_increase {
            self.bytes_allocated += abs_diff;
        } else {
            self.bytes_allocated -= abs_diff;
        }
        // Update in-block recorded size
        block.size = size;
        Ok(())
    }

    /// `REALLOC(parser, ptr, size)` of a block that may not exist yet (`ptr` null: `MALLOC`).
    pub(crate) fn realloc_opt(
        &mut self,
        block: &mut Option<Block>,
        size: usize,
    ) -> Result<(), XmlError> {
        match block {
            Some(b) => self.realloc(b, size),
            None => {
                *block = Some(self.malloc(size)?);
                Ok(())
            }
        }
    }
}

/// A pool block: its `size` (`BLOCK::size`, in `XML_Char`s) and its counted allocation.
#[derive(Debug)]
struct PoolBlock {
    size: i32,
    alloc: Block,
}

/// What a string pool's allocations depend on: its blocks, its free blocks, and where its
/// current string starts and ends in the current block. The bytes themselves live in the
/// port's own strings.
///
/// Port of `STRING_POOL` (xmlparse.c:354-361) and its functions (`poolInit`, `poolClear`,
/// `poolGrow`, `poolAppend`, `poolStoreString`, `poolCopyString`, `poolCopyStringN`,
/// `poolAppendString`, and the macros `poolAppendChar`, `poolFinish`, `poolDiscard`,
/// `poolChop`; xmlparse.c:643-652, 7943-8191), for `XML_Char` of one byte.
#[derive(Debug, Default)]
pub(crate) struct PoolModel {
    /// `blocks`: the head last.
    blocks: Vec<PoolBlock>,
    /// `freeBlocks`: the head last.
    free_blocks: Vec<PoolBlock>,
    /// `ptr != NULL`: `start`, `ptr` and `end` point into the head of `blocks`.
    has_ptr: bool,
    /// `start`, `ptr` and `end` as offsets from the head block's `s`.
    start: usize,
    ptr: usize,
    end: usize,
}

impl PoolModel {
    /// `pool->blocks != NULL`.
    pub(crate) fn has_blocks(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// `poolFinish(pool)`.
    pub(crate) fn finish(&mut self) {
        self.start = self.ptr;
    }

    /// `poolDiscard(pool)`.
    pub(crate) fn discard(&mut self) {
        self.ptr = self.start;
    }

    /// `poolChop(pool)`.
    pub(crate) fn chop(&mut self) {
        self.ptr -= 1;
    }

    /// Port of `poolClear` (xmlparse.c:7953-7970): the blocks become free blocks.
    pub(crate) fn clear(&mut self) {
        if self.free_blocks.is_empty() {
            self.free_blocks = std::mem::take(&mut self.blocks);
        } else {
            while let Some(p) = self.blocks.pop() {
                self.free_blocks.push(p);
            }
        }
        self.has_ptr = false;
        self.start = 0;
        self.ptr = 0;
        self.end = 0;
    }

    /// Port of `poolGrow` (xmlparse.c:8090-8191).
    pub(crate) fn grow(&mut self, tracker: &mut MallocTracker) -> Result<(), XmlError> {
        if let Some(free_size) = self.free_blocks.last().map(|b| b.size) {
            if !self.has_ptr {
                let head = self.free_blocks.pop().expect("a free block");
                self.blocks.push(head);
                self.has_ptr = true;
                self.start = 0;
                self.end = self.blocks.last().expect("a block").size as usize;
                self.ptr = 0;
                return Ok(());
            }
            if ((self.end - self.start) as i64) < free_size as i64 {
                let head = self.free_blocks.pop().expect("a free block");
                self.blocks.push(head);
                // memcpy of the current string to the new block.
                self.ptr -= self.start;
                self.start = 0;
                self.end = self.blocks.last().expect("a block").size as usize;
                return Ok(());
            }
        }
        if self.has_ptr && self.start == 0 && !self.blocks.is_empty() {
            let head_size = self.end - self.start;
            let block_size = (head_size as u32).wrapping_mul(2) as i32;
            if block_size < 0 {
                return Err(XmlError::NoMemory);
            }
            let bytes_to_allocate = pool_bytes_to_allocate_for(block_size);
            if bytes_to_allocate == 0 {
                return Err(XmlError::NoMemory);
            }
            let head = self.blocks.last_mut().expect("a block");
            tracker.realloc(&mut head.alloc, bytes_to_allocate as u32 as usize)?;
            head.size = block_size;
            self.end = block_size as usize;
        } else {
            let mut block_size = if self.has_ptr {
                (self.end - self.start) as i32
            } else {
                0
            };
            if block_size < 0 {
                return Err(XmlError::NoMemory);
            }
            if block_size < INIT_BLOCK_SIZE {
                block_size = INIT_BLOCK_SIZE;
            } else {
                // Detect overflow, avoiding _signed_ overflow undefined behavior
                if ((block_size as u32).wrapping_mul(2) as i32) < 0 {
                    return Err(XmlError::NoMemory);
                }
                block_size *= 2;
            }
            let bytes_to_allocate = pool_bytes_to_allocate_for(block_size);
            if bytes_to_allocate == 0 {
                return Err(XmlError::NoMemory);
            }
            let alloc = tracker.malloc(bytes_to_allocate)?;
            self.blocks.push(PoolBlock {
                size: block_size,
                alloc,
            });
            // memcpy of the current string to the new block.
            let len = if self.has_ptr {
                self.ptr - self.start
            } else {
                0
            };
            self.has_ptr = true;
            self.ptr = len;
            self.start = 0;
            self.end = block_size as usize;
        }
        Ok(())
    }

    /// `poolAppendChar(pool, c)` (xmlparse.c:649-652): one byte.
    pub(crate) fn append_char(&mut self, tracker: &mut MallocTracker) -> Result<(), XmlError> {
        // The macro tests ptr == end: both NULL before the first block.
        if !self.has_ptr || self.ptr == self.end {
            self.grow(tracker)?;
        }
        self.ptr += 1;
        Ok(())
    }

    /// `poolAppend(pool, enc, ptr, end)` (xmlparse.c:7988-8003) of text whose UTF-8 form is
    /// `utf8`: the conversion stores whole characters only, so a character that doesn't fit
    /// in the block makes it grow.
    pub(crate) fn append(
        &mut self,
        tracker: &mut MallocTracker,
        utf8: &[u8],
    ) -> Result<(), XmlError> {
        if !self.has_ptr {
            self.grow(tracker)?;
        }
        let mut i = 0usize;
        loop {
            // XmlConvert: as many whole characters as fit before `end`.
            while i < utf8.len() {
                let n = utf8_char_len(utf8[i]).min(utf8.len() - i);
                if self.end - self.ptr < n {
                    break;
                }
                self.ptr += n;
                i += n;
            }
            if i == utf8.len() {
                break;
            }
            self.grow(tracker)?;
        }
        Ok(())
    }

    /// `poolStoreString(pool, enc, ptr, end)` (xmlparse.c:8051-8060): the text and a NUL.
    pub(crate) fn store_string(
        &mut self,
        tracker: &mut MallocTracker,
        utf8: &[u8],
    ) -> Result<(), XmlError> {
        self.append(tracker, utf8)?;
        if self.ptr == self.end {
            self.grow(tracker)?;
        }
        self.ptr += 1;
        Ok(())
    }

    /// `poolCopyString(pool, s)` (xmlparse.c:8005-8014): the string, its NUL, then
    /// `poolFinish`.
    pub(crate) fn copy_string(
        &mut self,
        tracker: &mut MallocTracker,
        len: usize,
    ) -> Result<(), XmlError> {
        for _ in 0..=len {
            self.append_char(tracker)?;
        }
        self.finish();
        Ok(())
    }

    /// `poolAppendString(pool, s)` (xmlparse.c:8041-8049): `len` bytes, no NUL.
    pub(crate) fn append_string(
        &mut self,
        tracker: &mut MallocTracker,
        len: usize,
    ) -> Result<(), XmlError> {
        for _ in 0..len {
            self.append_char(tracker)?;
        }
        Ok(())
    }
}

/// The length of the UTF-8 character that starts with `lead` (1 for a byte that doesn't start
/// one), as `_INTERNAL_trim_to_complete_utf8_characters` reads it.
fn utf8_char_len(lead: u8) -> usize {
    if lead & 0x80 == 0 {
        1
    } else if lead & 0xE0 == 0xC0 {
        2
    } else if lead & 0xF0 == 0xE0 {
        3
    } else if lead & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

/// Port of `poolBytesToAllocateFor` (xmlparse.c:8062-8088) for `XML_Char` of one byte: 0 for
/// a size that can't be allocated.
fn pool_bytes_to_allocate_for(block_size: i32) -> usize {
    if block_size <= 0 {
        return 0;
    }
    let bytes_to_allocate = (OFFSETOF_BLOCK_S as u32).wrapping_add(block_size as u32) as i32;
    if bytes_to_allocate < 0 {
        return 0;
    }
    bytes_to_allocate as usize
}

/// What a hash table's allocations depend on: its array and its entries.
///
/// Port of `HASH_TABLE` (xmlparse.c:232-238) and of the allocations of `lookup`
/// (xmlparse.c:7811-7898) and `hashTableClear` (7900-7908).
#[derive(Debug, Default)]
pub(crate) struct HashModel {
    /// `power`.
    power: u8,
    /// `size`.
    size: usize,
    /// `used`.
    used: usize,
    /// `v`.
    v: Option<Block>,
    /// The entries' blocks.
    entries: Vec<Block>,
}

impl HashModel {
    /// The allocations of `lookup(parser, table, name, createSize)` for a name not in the
    /// table and a `createSize` not 0: the array on the first insertion, a bigger one when
    /// half full (allocated before the old one is freed), then the entry.
    pub(crate) fn insert(
        &mut self,
        tracker: &mut MallocTracker,
        create_size: usize,
    ) -> Result<(), XmlError> {
        if self.size == 0 {
            self.power = INIT_POWER;
            // table->size is a power of 2
            self.size = 1usize << INIT_POWER;
            let tsize = self.size * SIZEOF_NAMED_PTR;
            match tracker.malloc(tsize) {
                Ok(v) => self.v = Some(v),
                Err(e) => {
                    self.size = 0;
                    return Err(e);
                }
            }
        } else if (self.used >> (self.power - 1)) != 0 {
            // check for overflow (table is half full)
            let new_power = self.power + 1;
            let new_size = 1usize << new_power;
            let tsize = new_size * SIZEOF_NAMED_PTR;
            let new_v = tracker.malloc(tsize)?;
            if let Some(old) = self.v.replace(new_v) {
                tracker.free(old);
            }
            self.power = new_power;
            self.size = new_size;
        }
        let entry = tracker.malloc(create_size)?;
        self.entries.push(entry);
        self.used += 1;
        Ok(())
    }

    /// Port of `hashTableClear` (xmlparse.c:7900-7908): the entries are freed, the array
    /// kept.
    pub(crate) fn clear(&mut self, tracker: &mut MallocTracker) {
        for entry in self.entries.drain(..) {
            tracker.free(entry);
        }
        self.used = 0;
    }
}

/// An open tag's blocks: the `TAG` and its name buffer (`buf` to `bufEnd`).
#[derive(Debug)]
struct TagMem {
    /// The `TAG`.
    _tag: Block,
    /// `buf`, whose size is `bufEnd - buf`.
    buf: Block,
    /// `name.strLen`: the name's length in UTF-8.
    str_len: usize,
    /// `rawNameLength`.
    raw_name_length: usize,
    /// `rawName == buf + nameLen`: the raw name was copied into `buf` (`storeRawNames`).
    raw_stored: bool,
}

/// The lists of open entities the document reaches: `ENTITY_INTERNAL` and `ENTITY_ATTRIBUTE`
/// (`ENTITY_VALUE`'s, opened only from parameter entities, is not).
#[derive(Debug, Clone, Copy)]
pub(crate) enum EntityList {
    Internal = 0,
    Attribute = 1,
}

/// The string pools: `m_tempPool`, `m_temp2Pool`, and the DTD's `pool` and `entityValuePool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoolId {
    Temp,
    Temp2,
    Dtd,
    EntityValue,
}

/// Everything the tracker counts, beside the parser's own structures.
#[derive(Debug)]
pub(crate) struct Heap {
    /// `m_alloc_tracker`.
    pub(crate) tracker: MallocTracker,
    /// `m_atts` (`m_attsSize` `ATTRIBUTE`s).
    atts: Block,
    /// `m_dataBuf`.
    _data_buf: Block,
    /// The `DTD`.
    _dtd: Block,
    /// `m_tempPool`, `m_temp2Pool`.
    pub(crate) temp_pool: PoolModel,
    pub(crate) temp2_pool: PoolModel,
    /// The DTD's `pool` and `entityValuePool`.
    pub(crate) dtd_pool: PoolModel,
    pub(crate) entity_value_pool: PoolModel,
    /// The DTD's hash tables.
    pub(crate) general_entities: HashModel,
    pub(crate) param_entities: HashModel,
    pub(crate) element_types: HashModel,
    pub(crate) attribute_ids: HashModel,
    pub(crate) prefixes: HashModel,
    /// `m_tagStack` (the innermost last) and `m_freeTagList` (its head last).
    tag_stack: Vec<TagMem>,
    free_tags: Vec<TagMem>,
    /// The open entities of each list, and the free ones (heads last).
    open_entities: [Vec<Block>; 2],
    free_entities: [Vec<Block>; 2],
    /// Each element type's `defaultAtts` and `allocDefaultAtts`, by the port's index.
    default_atts: Vec<Option<(Block, usize)>>,
    /// `m_groupConnector`.
    pub(crate) group_connector: Option<Block>,
}

impl Heap {
    /// The parser's first allocations: the `XML_ParserStruct` (counted without
    /// `expat_malloc`), `m_atts`, `m_dataBuf`, then the `DTD`.
    ///
    /// Port of the allocations of `parserCreate` (xmlparse.c:1333-1523: 1340-1426,
    /// 1435-1466) and `dtdCreate` (7474-7505).
    pub(crate) fn new(tracker: MallocTracker) -> Heap {
        let mut tracker = tracker;
        // Record XML_ParserStruct allocation we did a few lines up before
        tracker.bytes_allocated += SIZE_T + SIZEOF_PARSER;
        // Nothing is parsed yet: the default thresholds can't be reached.
        let atts = tracker
            .malloc(16 * SIZEOF_ATTRIBUTE)
            .expect("the first allocations");
        let data_buf = tracker.malloc(1024).expect("the first allocations");
        let dtd = tracker.malloc(SIZEOF_DTD).expect("the first allocations");
        Heap {
            tracker,
            atts,
            _data_buf: data_buf,
            _dtd: dtd,
            temp_pool: PoolModel::default(),
            temp2_pool: PoolModel::default(),
            dtd_pool: PoolModel::default(),
            entity_value_pool: PoolModel::default(),
            general_entities: HashModel::default(),
            param_entities: HashModel::default(),
            element_types: HashModel::default(),
            attribute_ids: HashModel::default(),
            prefixes: HashModel::default(),
            tag_stack: Vec::new(),
            free_tags: Vec::new(),
            open_entities: Default::default(),
            free_entities: Default::default(),
            default_atts: Vec::new(),
            group_connector: None,
        }
    }

    /// The allocations of `XML_ParserReset` (xmlparse.c:1626-1683): the open tags and
    /// entities go to the free lists, the temporary pools are cleared, the accounting starts
    /// over, and `dtdReset` (7506-7545) frees the default attributes and the hash tables'
    /// entries (keeping their arrays) and clears the DTD's pools.
    pub(crate) fn reset(&mut self) {
        while let Some(tag) = self.tag_stack.pop() {
            self.free_tags.push(tag);
        }
        for list in 0..2 {
            while let Some(entity) = self.open_entities[list].pop() {
                self.free_entities[list].push(entity);
            }
        }
        self.temp_pool.clear();
        self.temp2_pool.clear();
        // parserInit: the accounting starts over.
        self.tracker.count_bytes_direct = 0;
        for (block, _) in self.default_atts.drain(..).flatten() {
            self.tracker.free(block);
        }
        self.general_entities.clear(&mut self.tracker);
        self.param_entities.clear(&mut self.tracker);
        self.element_types.clear(&mut self.tracker);
        self.attribute_ids.clear(&mut self.tracker);
        self.prefixes.clear(&mut self.tracker);
        self.dtd_pool.clear();
        self.entity_value_pool.clear();
    }

    /// A pool, and the tracker its blocks are counted by.
    pub(crate) fn pool(&mut self, id: PoolId) -> (&mut PoolModel, &mut MallocTracker) {
        let pool = match id {
            PoolId::Temp => &mut self.temp_pool,
            PoolId::Temp2 => &mut self.temp2_pool,
            PoolId::Dtd => &mut self.dtd_pool,
            PoolId::EntityValue => &mut self.entity_value_pool,
        };
        (pool, &mut self.tracker)
    }

    /// The `m_atts` reallocation of `storeAtts` (xmlparse.c:3850-3882): `size` `ATTRIBUTE`s.
    pub(crate) fn realloc_atts(&mut self, size: usize) -> Result<(), XmlError> {
        self.tracker
            .realloc(&mut self.atts, size * SIZEOF_ATTRIBUTE)
    }

    /// A start tag's `TAG` and name buffer (xmlparse.c:3451-3505): one of the free list, or a
    /// new `TAG` with a buffer of `INIT_TAG_BUF_SIZE` (32) bytes; the buffer doubles until the
    /// name, `str_len` bytes in UTF-8, fits with a byte to spare.
    pub(crate) fn push_tag(
        &mut self,
        str_len: usize,
        raw_name_length: usize,
    ) -> Result<(), XmlError> {
        let mut tag = match self.free_tags.pop() {
            Some(tag) => tag,
            None => {
                let tag = self.tracker.malloc(SIZEOF_TAG)?;
                let buf = match self.tracker.malloc(INIT_TAG_BUF_SIZE) {
                    Ok(buf) => buf,
                    Err(e) => {
                        self.tracker.free(tag);
                        return Err(e);
                    }
                };
                TagMem {
                    _tag: tag,
                    buf,
                    str_len: 0,
                    raw_name_length: 0,
                    raw_stored: false,
                }
            }
        };
        tag.str_len = 0;
        tag.raw_name_length = raw_name_length;
        tag.raw_stored = false;
        // The tag is on the stack before its name is converted.
        self.tag_stack.push(tag);
        // XmlConvert into buf .. bufEnd - 1: whole characters, so the name fits once its
        // length is at most bufEnd - buf - 1.
        loop {
            let tag = self.tag_stack.last_mut().expect("pushed");
            if str_len < tag.buf.size {
                tag.str_len = str_len;
                return Ok(());
            }
            let buf_size = ((tag.buf.size as i32) << 1) as usize;
            self.tracker.realloc(&mut tag.buf, buf_size)?;
        }
    }

    /// An end tag: the innermost tag goes to the free list (xmlparse.c:3580-3582).
    pub(crate) fn pop_tag(&mut self) {
        let tag = self.tag_stack.pop().expect("an open tag");
        self.free_tags.push(tag);
    }

    /// Port of the allocations of `storeRawNames` (xmlparse.c:3111-3158): from the innermost
    /// tag to the first whose raw name is already in its buffer, the buffer grows to hold the
    /// name, its NUL and the raw name.
    pub(crate) fn store_raw_names(&mut self) -> Result<(), XmlError> {
        for tag in self.tag_stack.iter_mut().rev() {
            if tag.raw_stored {
                break;
            }
            let name_len = tag.str_len + 1;
            let raw_name_len = tag.raw_name_length;
            // Detect and prevent integer overflow.
            if raw_name_len > i32::MAX as usize - name_len {
                return Err(XmlError::NoMemory);
            }
            let buf_size = name_len + raw_name_len;
            if buf_size > tag.buf.size {
                self.tracker.realloc(&mut tag.buf, buf_size)?;
            }
            tag.raw_stored = true;
        }
        Ok(())
    }

    /// An entity opened on `list` (`processEntity`, xmlparse.c:6358-6365): an
    /// `OPEN_INTERNAL_ENTITY` of the list's free list, or a new one.
    pub(crate) fn open_entity(&mut self, list: EntityList) -> Result<(), XmlError> {
        let block = match self.free_entities[list as usize].pop() {
            Some(block) => block,
            None => self.tracker.malloc(SIZEOF_OPEN_INTERNAL_ENTITY)?,
        };
        self.open_entities[list as usize].push(block);
        Ok(())
    }

    /// The innermost entity on `list` closed: its `OPEN_INTERNAL_ENTITY` goes to the free list.
    pub(crate) fn close_entity(&mut self, list: EntityList) {
        let block = self.open_entities[list as usize]
            .pop()
            .expect("an open entity");
        self.free_entities[list as usize].push(block);
    }

    /// Room for one more default attribute of the element type `index`, which has `n` (the
    /// allocations of `defineAttribute`, xmlparse.c:7154-7190): 8 at first, then twice as
    /// many.
    pub(crate) fn grow_default_atts(&mut self, index: usize, n: usize) -> Result<(), XmlError> {
        if self.default_atts.len() <= index {
            self.default_atts.resize_with(index + 1, || None);
        }
        let alloc = self.default_atts[index]
            .as_ref()
            .map_or(0, |(_, alloc)| *alloc);
        if n == alloc {
            if alloc == 0 {
                let block = self.tracker.malloc(8 * SIZEOF_DEFAULT_ATTRIBUTE)?;
                self.default_atts[index] = Some((block, 8));
            } else {
                // Detect and prevent integer overflow
                if alloc > i32::MAX as usize / 2 {
                    return Err(XmlError::NoMemory);
                }
                let count = alloc * 2;
                let (block, alloc) = self.default_atts[index].as_mut().expect("allocated");
                self.tracker
                    .realloc(block, count * SIZEOF_DEFAULT_ATTRIBUTE)?;
                *alloc = count;
            }
        }
        Ok(())
    }
}
