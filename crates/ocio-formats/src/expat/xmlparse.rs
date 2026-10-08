// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The parser: a port of the parts of `lib/xmlparse.c` and `expat.h` (expat 2.7.2) that a
//! parser created without namespace processing reaches with the handlers OCIO sets.
//!
//! **What is ported.** `XML_ParserCreate`, `XML_ParserReset`, `XML_Parse`, `XML_GetBuffer`,
//! `XML_ParseBuffer`, `XML_GetErrorCode`, `XML_ErrorString`, `XML_GetCurrentLineNumber`,
//! `XML_GetCurrentColumnNumber`, `XML_GetCurrentByteIndex`, `XML_GetSpecifiedAttributeCount`,
//! `XML_GetIdAttributeIndex`, `XML_SetReparseDeferralEnabled`, and the element and character
//! data handlers; with them the whole of document parsing: the encodings and the XML
//! declaration, the prolog and its internal DTD subset (entity, attribute list, element and
//! notation declarations, attribute defaults), the content (tags, attributes and their
//! normalization, references to the predefined, character and internal general entities,
//! CDATA sections, comments and processing instructions), the epilog, the reparse deferral
//! heuristic and the amplification accounting (`XML_GE`: the billion laughs protection).
//!
//! **What is not.** OCIO creates its parsers with `XML_ParserCreate(nullptr)` and sets only
//! the start element, end element and character data handlers (`FileFormatCTF.cpp:179-184`,
//! `CDLParser.cpp:163-316`). So no code here is reached by: namespace processing
//! (`XML_ParserCreateNS`), a protocol encoding (`XML_SetEncoding`), the other handlers
//! (default, comment, processing instruction, CDATA section, doctype, entity, notation,
//! attribute list, element declaration, XML declaration, skipped entity, not-standalone,
//! unknown encoding, external entity reference: where upstream tests for one and finds none,
//! the port takes the path without it), external entity parsers and parameter entity
//! parsing (`XML_SetParamEntityParsing` stays `NEVER`, so a parameter entity reference only
//! resets `keepProcessing`), the content model scaffold (built only for an element
//! declaration handler), `XML_StopParser` (so the parser is never suspended or finished by a
//! handler), and the debug reports to `stderr` (`EXPAT_*_DEBUG`). The prolog's text
//! declaration and ignore sections belong to external entities, and are not reached either.
//!
//! **The allocation tracker** (`MALLOC_TRACKER`, new in 2.7.2) fails an allocation with
//! `XML_ERROR_NO_MEMORY` once expat's own heap use passes 64 MiB at more than 100 times the
//! document's direct bytes. Its rule, limits and setters are ported; what it counts is the
//! port's own structures, charged where expat allocates (the parser, the DTD's entries,
//! names, entity texts and default attributes, the tag stack, the attribute arrays and
//! values, the open entities, the group connectors), so the document where the limit starts
//! differs from the wheel's (owner decision 2026-10-08, `docs/improvements.md` I-171).
//!
//! **Memory.** Upstream keeps strings in pools and positions as pointers; the port owns its
//! strings and keeps positions as indices into the text being parsed (the parse buffer, or an
//! entity's replacement text). A tag's raw name is copied when the tag opens, so
//! `storeRawNames` has nothing left to do. Hash tables are maps: expat's salted hashes order
//! nothing visible. The port's allocations don't fail, so the `XML_ERROR_NO_MEMORY` returns
//! of failed allocations are not ported; the ones of size checks are.
//!
//! **Handlers and errors.** A handler returns `Result<(), E>`. OCIO's handlers throw C++
//! exceptions, which unwind through expat and end the parse; a handler's `Err` likewise ends
//! the call that reached it, and [`Parser::parse`] returns it ([`ParseError::Handler`]).
//! OCIO never uses a parser again after one of its handlers threw, and neither should a
//! caller of the port.

use std::collections::HashMap;
use std::rc::Rc;

use super::xmlrole::*;
use super::xmltok::{
    Attribute, ConvertResult, Encoding, InitEncoding, Position, XML_PROLOG_STATE,
    XML_UTF8_ENCODE_MAX, xml_get_utf8_internal_encoding, xml_parse_xml_decl, xml_utf8_encode,
};
use super::xmltok_impl::*;

/// An error of the parser.
///
/// Port of `enum XML_Error` (expat.h:83-136).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum XmlError {
    None = 0,
    NoMemory,
    Syntax,
    NoElements,
    InvalidToken,
    UnclosedToken,
    PartialChar,
    TagMismatch,
    DuplicateAttribute,
    JunkAfterDocElement,
    ParamEntityRef,
    UndefinedEntity,
    RecursiveEntityRef,
    AsyncEntity,
    BadCharRef,
    BinaryEntityRef,
    AttributeExternalEntityRef,
    MisplacedXmlPi,
    UnknownEncoding,
    IncorrectEncoding,
    UnclosedCdataSection,
    ExternalEntityHandling,
    NotStandalone,
    UnexpectedState,
    EntityDeclaredInPe,
    FeatureRequiresXmlDtd,
    CantChangeFeatureOnceParsing,
    UnboundPrefix,
    UndeclaringPrefix,
    IncompletePe,
    XmlDecl,
    TextDecl,
    Publicid,
    Suspended,
    NotSuspended,
    Aborted,
    Finished,
    SuspendPe,
    ReservedPrefixXml,
    ReservedPrefixXmlns,
    ReservedNamespaceUri,
    InvalidArgument,
    NoBuffer,
    AmplificationLimitBreach,
    NotStarted,
}

/// The message of an error code, or `None` for `XML_ERROR_NONE`.
///
/// Port of `XML_ErrorString` (xmlparse.c:2855-2961).
pub fn xml_error_string(code: XmlError) -> Option<&'static str> {
    use XmlError::*;
    Some(match code {
        None => return Option::None,
        NoMemory => "out of memory",
        Syntax => "syntax error",
        NoElements => "no element found",
        InvalidToken => "not well-formed (invalid token)",
        UnclosedToken => "unclosed token",
        PartialChar => "partial character",
        TagMismatch => "mismatched tag",
        DuplicateAttribute => "duplicate attribute",
        JunkAfterDocElement => "junk after document element",
        ParamEntityRef => "illegal parameter entity reference",
        UndefinedEntity => "undefined entity",
        RecursiveEntityRef => "recursive entity reference",
        AsyncEntity => "asynchronous entity",
        BadCharRef => "reference to invalid character number",
        BinaryEntityRef => "reference to binary entity",
        AttributeExternalEntityRef => "reference to external entity in attribute",
        MisplacedXmlPi => "XML or text declaration not at start of entity",
        UnknownEncoding => "unknown encoding",
        IncorrectEncoding => "encoding specified in XML declaration is incorrect",
        UnclosedCdataSection => "unclosed CDATA section",
        ExternalEntityHandling => "error in processing external entity reference",
        NotStandalone => "document is not standalone",
        UnexpectedState => "unexpected parser state - please send a bug report",
        EntityDeclaredInPe => "entity declared in parameter entity",
        FeatureRequiresXmlDtd => "requested feature requires XML_DTD support in Expat",
        CantChangeFeatureOnceParsing => "cannot change setting once parsing has begun",
        UnboundPrefix => "unbound prefix",
        UndeclaringPrefix => "must not undeclare prefix",
        IncompletePe => "incomplete markup in parameter entity",
        XmlDecl => "XML declaration not well-formed",
        TextDecl => "text declaration not well-formed",
        Publicid => "illegal character(s) in public id",
        Suspended => "parser suspended",
        NotSuspended => "parser not suspended",
        Aborted => "parsing aborted",
        Finished => "parsing finished",
        SuspendPe => "cannot suspend in external parameter entity",
        ReservedPrefixXml => {
            "reserved prefix (xml) must not be undeclared or bound to another namespace name"
        }
        ReservedPrefixXmlns => "reserved prefix (xmlns) must not be declared or undeclared",
        ReservedNamespaceUri => "prefix must not be bound to one of the reserved namespace names",
        InvalidArgument => "invalid argument",
        NoBuffer => "a successful prior call to function XML_GetBuffer is required",
        AmplificationLimitBreach => {
            "limit on input amplification factor (from DTD and entities) breached"
        }
        NotStarted => "parser not started",
    })
}

/// The status of a parse call.
///
/// Port of `enum XML_Status` (expat.h:74-81), without `XML_STATUS_SUSPENDED`, which needs
/// `XML_StopParser`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlStatus {
    /// `XML_STATUS_ERROR`: the error code says why.
    Error,
    /// `XML_STATUS_OK`.
    Ok,
}

/// How a parse call ended without an XML status: a handler's error, which ends the parse as
/// OCIO's C++ exceptions do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError<E> {
    /// A handler returned `Err`.
    Handler(E),
}

/// `enum XML_Parsing` (expat.h:845), without `XML_SUSPENDED`, which needs
/// `XML_StopParser`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsingStatus {
    Initialized,
    Parsing,
    Finished,
}

/// `enum XML_Account` (xmlparse.c:438-443).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Account {
    /// `XML_ACCOUNT_DIRECT`: bytes directly passed to the parser.
    Direct,
    /// `XML_ACCOUNT_ENTITY_EXPANSION`: intermediate bytes produced during entity expansion.
    EntityExpansion,
    /// `XML_ACCOUNT_NONE`: do not account, was accounted already.
    None,
}

/// `enum EntityType` (xmlparse.c:422-426), without `ENTITY_VALUE`: value entities are
/// parameter entities in entity values, which are never parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityType {
    Internal,
    Attribute,
}

/// The parser's processor: upstream's function pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Processor {
    PrologInit,
    Prolog,
    Content,
    CdataSection,
    Epilog,
    InternalEntity,
    Error,
}

/// The parser's encoding: the initial one until the document's is known
/// (`parser->m_encoding`, which points at `m_initEncoding.initEnc` until then).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncState {
    Init,
    Known(&'static Encoding),
}

/// Where a scan reads: the parse buffer in the parser's encoding, or the replacement text of
/// the open internal entity at this index (in the internal encoding). Upstream tells them
/// apart by comparing `enc` with `parser->m_encoding`, and keeps their event positions in the
/// parser (`m_eventPtr`) or in the open entity (`internalEventPtr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Src {
    Main,
    Entity(usize),
}

/// A declared entity.
///
/// Port of `ENTITY` (xmlparse.c:318-334), without the fields only handlers read (`systemId`,
/// `base`, `publicId`, and the notation's name).
#[derive(Debug, Clone, Default)]
struct Entity {
    /// `textPtr`, `textLen`: the replacement text of an internal entity (UTF-8).
    text: Option<Rc<[u8]>>,
    /// `processed`: the bytes of the text processed so far.
    processed: usize,
    /// `notation != NULL`: an unparsed entity.
    has_notation: bool,
    /// `open`.
    open: bool,
    /// `hasMore`: the entity has not been completely processed.
    has_more: bool,
    /// `is_internal`: declared in the internal subset outside a parameter entity.
    is_internal: bool,
}

/// An attribute name.
///
/// Port of `ATTRIBUTE_ID` (xmlparse.c:365-370) without namespace processing; `name[-1]`, the
/// byte before the name that marks it specified on the current tag, is `specified`.
#[derive(Debug, Clone)]
struct AttributeId {
    name: Rc<[u8]>,
    /// `name[-1]`.
    specified: bool,
    /// `maybeTokenized`: declared other than CDATA somewhere.
    maybe_tokenized: bool,
}

/// A default attribute of an element type.
///
/// Port of `DEFAULT_ATTRIBUTE` (xmlparse.c:372-376).
#[derive(Debug, Clone)]
struct DefaultAttribute {
    id: usize,
    is_cdata: bool,
    value: Option<Rc<[u8]>>,
}

/// An element type.
///
/// Port of `ELEMENT_TYPE` (xmlparse.c:384-391), without the namespace prefix.
#[derive(Debug, Clone, Default)]
struct ElementType {
    /// `idAtt`.
    id_att: Option<usize>,
    /// `defaultAtts`, `nDefaultAtts`.
    default_atts: Vec<DefaultAttribute>,
}

/// The DTD.
///
/// Port of `DTD` (xmlparse.c:393-420), without the namespace prefixes, the content model
/// scaffold and `paramEntityRead` (read only after an external entity handler). The hash
/// tables are maps from names to indices into the arenas.
#[derive(Debug, Clone)]
struct Dtd {
    general_entities: HashMap<Vec<u8>, usize>,
    param_entities: HashMap<Vec<u8>, usize>,
    entities: Vec<Entity>,
    element_types: HashMap<Vec<u8>, usize>,
    elements: Vec<ElementType>,
    attribute_ids: HashMap<Vec<u8>, usize>,
    att_ids: Vec<AttributeId>,
    /// `keepProcessing`: false once a parameter entity reference has been skipped.
    keep_processing: bool,
    /// `hasParamEntityRefs`: true once a parameter entity reference has been encountered,
    /// the reference to an external subset included.
    has_param_entity_refs: bool,
    /// `standalone`.
    standalone: bool,
}

impl Dtd {
    /// Port of `dtdCreate` (xmlparse.c:7474-7505).
    fn new() -> Dtd {
        Dtd {
            general_entities: HashMap::new(),
            param_entities: HashMap::new(),
            entities: Vec::new(),
            element_types: HashMap::new(),
            elements: Vec::new(),
            attribute_ids: HashMap::new(),
            att_ids: Vec::new(),
            keep_processing: true,
            has_param_entity_refs: false,
            standalone: false,
        }
    }

    /// `lookup(parser, table, name, sizeof(ENTITY))` (xmlparse.c:7811-7898) on the general or
    /// the parameter entities: the entity `name`, made if missing; and whether it was made.
    fn lookup_entity(&mut self, param: bool, name: &[u8]) -> (usize, bool) {
        let table = if param {
            &mut self.param_entities
        } else {
            &mut self.general_entities
        };
        if let Some(&id) = table.get(name) {
            return (id, false);
        }
        let id = self.entities.len();
        self.entities.push(Entity::default());
        table.insert(name.to_vec(), id);
        (id, true)
    }

    /// `lookup(parser, &dtd->elementTypes, name, sizeof(ELEMENT_TYPE))`: the element type
    /// `name`, made if missing.
    fn lookup_element_type(&mut self, name: &[u8]) -> usize {
        if let Some(&id) = self.element_types.get(name) {
            return id;
        }
        let id = self.elements.len();
        self.elements.push(ElementType::default());
        self.element_types.insert(name.to_vec(), id);
        id
    }
}

/// An open element.
///
/// Port of `TAG` (xmlparse.c:308-316): its name in the encoding of the text it was read from,
/// and in UTF-8.
#[derive(Debug, Clone)]
struct Tag {
    /// `rawName`, `rawNameLength`.
    raw_name: Vec<u8>,
    /// `name.str`.
    name: Vec<u8>,
}

/// An entity being expanded.
///
/// Port of `OPEN_INTERNAL_ENTITY` (xmlparse.c:428-436), without `betweenDecl` (read only for
/// parameter entities) and `type` (the list it is on says it).
#[derive(Debug, Clone)]
struct OpenEntity {
    /// `internalEventPtr`, `internalEventEndPtr`: positions in the entity's text.
    internal_event_ptr: Option<usize>,
    internal_event_end_ptr: Option<usize>,
    /// `entity`.
    entity: usize,
    /// `startTagLevel`.
    start_tag_level: i32,
}

/// The billion laughs protection's counts.
///
/// Port of `ACCOUNTING` (xmlparse.c:447-453), without the debug level.
#[derive(Debug, Clone, Copy)]
struct Accounting {
    count_bytes_direct: u64,
    count_bytes_indirect: u64,
    maximum_amplification_factor: f32,
    activation_threshold_bytes: u64,
}

/// `EXPAT_BILLION_LAUGHS_ATTACK_PROTECTION_MAXIMUM_AMPLIFICATION_DEFAULT` (internal.h:147).
const MAXIMUM_AMPLIFICATION_DEFAULT: f32 = 100.0f32;
/// `EXPAT_BILLION_LAUGHS_ATTACK_PROTECTION_ACTIVATION_THRESHOLD_DEFAULT` (internal.h:149): 8 MiB.
const ACTIVATION_THRESHOLD_DEFAULT: u64 = 8388608;

/// `EXPAT_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION_DEFAULT` (internal.h:152).
const ALLOC_TRACKER_MAXIMUM_AMPLIFICATION_DEFAULT: f32 = 100.0f32;
/// `EXPAT_ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT` (internal.h:153-154): 64 MiB.
const ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT: u64 = 67108864;

/// The allocation tracker.
///
/// Port of `MALLOC_TRACKER` (xmlparse.c:455-461), without the debug fields.
#[derive(Debug, Clone, Copy)]
struct MallocTracker {
    /// `bytesAllocated`.
    bytes_allocated: u64,
    /// `maximumAmplificationFactor`.
    maximum_amplification_factor: f32,
    /// `activationThresholdBytes`.
    activation_threshold_bytes: u64,
}

/// A block `expat_malloc` counted: the size it recorded in the `size_t` before the block.
#[derive(Debug)]
struct Block {
    size: usize,
}

/// `INIT_TAG_BUF_SIZE` (xmlparse.c:262).
const INIT_TAG_BUF_SIZE: usize = 32;
/// `INIT_DATA_BUF_SIZE` (xmlparse.c:263): the buffer character data is converted into.
const INIT_DATA_BUF_SIZE: usize = 1024;
/// `INIT_ATTS_SIZE` (xmlparse.c:264).
const INIT_ATTS_SIZE: usize = 16;
/// `INIT_BUFFER_SIZE` (xmlparse.c:267).
const INIT_BUFFER_SIZE: i32 = 1024;
/// `XML_CONTEXT_BYTES`: the bytes before the parse position kept in the buffer.
const XML_CONTEXT_BYTES: i32 = 1024;

/// A start element handler: the element's name, and its attributes' names and values
/// alternately (expat's `atts`, without the null at its end).
pub type StartElementHandler<'a, E> = Box<dyn FnMut(&[u8], &[&[u8]]) -> Result<(), E> + 'a>;
/// An end element handler: the element's name.
pub type EndElementHandler<'a, E> = Box<dyn FnMut(&[u8]) -> Result<(), E> + 'a>;
/// A character data handler: UTF-8 bytes, not null-terminated.
pub type CharacterDataHandler<'a, E> = Box<dyn FnMut(&[u8]) -> Result<(), E> + 'a>;

/// Results of the functions that can call a handler: an XML error code (`XmlError::None` for
/// none), or a handler's error.
type HResult<E> = Result<XmlError, E>;

/// Returns from the enclosing function if the XML result is an error.
macro_rules! try_xml {
    ($e:expr) => {{
        let r = $e?;
        if r != XmlError::None {
            return Ok(r);
        }
    }};
}

/// `poolAppend(pool, enc, ptr, end)` (xmlparse.c:7988-8003): appends `b[ptr..end]`, converted
/// to UTF-8, to `out`; a partial character at the end is left out.
fn pool_append(enc: &Encoding, b: &[u8], ptr: usize, end: usize, out: &mut Vec<u8>) {
    let mut from = ptr;
    loop {
        let mut buf = [0u8; 256];
        let mut to = 0usize;
        let convert_res = enc.utf8_convert(b, &mut from, end, &mut buf, &mut to, 256);
        out.extend_from_slice(&buf[..to]);
        if convert_res == ConvertResult::Completed || convert_res == ConvertResult::InputIncomplete
        {
            break;
        }
    }
}

/// `poolStoreString(pool, enc, ptr, end)` (xmlparse.c:8051-8060) without the null: `b[ptr..end]`
/// converted to UTF-8.
fn pool_store_string(enc: &Encoding, b: &[u8], ptr: usize, end: usize) -> Vec<u8> {
    let mut out = Vec::new();
    pool_append(enc, b, ptr, end, &mut out);
    out
}

/// `MUST_CONVERT(enc, s)` (xmlparse.c:183) without `XML_UNICODE`.
fn must_convert(enc: &Encoding) -> bool {
    !enc.is_utf8
}

/// An expat parser, as `XML_ParserCreate(NULL)` makes it.
///
/// Port of `struct XML_ParserStruct` (xmlparse.c:663-787), without the parts the module notes
/// leave out.
pub struct Parser<'a, E> {
    start_element_handler: Option<StartElementHandler<'a, E>>,
    end_element_handler: Option<EndElementHandler<'a, E>>,
    character_data_handler: Option<CharacterDataHandler<'a, E>>,

    /// `m_buffer` .. `m_bufferLim`: `None` until the first `XML_GetBuffer`.
    buffer: Option<Vec<u8>>,
    /// `m_bufferPtr`: the first byte to be parsed.
    buffer_ptr: usize,
    /// `m_bufferEnd`: past the last byte to be parsed.
    buffer_end: usize,
    /// `m_parseEndByteIndex`.
    parse_end_byte_index: u64,
    /// `m_parseEndPtr`.
    parse_end_ptr: usize,
    /// `m_partialTokenBytesBefore`: used in heuristic to avoid O(n^2).
    partial_token_bytes_before: usize,
    /// `m_reparseDeferralEnabled`.
    reparse_deferral_enabled: bool,
    /// `g_reparseDeferralEnabledDefault` when the parser was made: `XML_ParserReset` restores
    /// it.
    reparse_deferral_default: bool,
    /// `m_lastBufferRequestSize`.
    last_buffer_request_size: i32,

    /// `m_encoding`.
    encoding: EncState,
    /// `m_initEncoding`.
    init_encoding: InitEncoding,
    /// `m_internalEncoding`.
    internal_encoding: &'static Encoding,

    /// `m_prologState`.
    prolog_state: PrologState,
    /// `m_processor`.
    processor: Processor,
    /// `m_errorCode`.
    error_code: XmlError,
    /// `m_eventPtr`, `m_eventEndPtr`, `m_positionPtr`: positions in the parse buffer.
    event_ptr: Option<usize>,
    event_end_ptr: Option<usize>,
    position_ptr: Option<usize>,
    /// `m_openInternalEntities` (the head last).
    open_internal_entities: Vec<OpenEntity>,
    /// `m_openAttributeEntities` (the head last).
    open_attribute_entities: Vec<OpenEntity>,
    /// `m_tagLevel`.
    tag_level: i32,
    /// `m_declEntity`.
    decl_entity: Option<usize>,
    /// `m_doctypeSysid != NULL` (without a doctype handler, upstream only tests it).
    doctype_sysid: bool,
    /// `m_declElementType`.
    decl_element_type: Option<usize>,
    /// `m_declAttributeId`.
    decl_attribute_id: Option<usize>,
    /// `m_declAttributeIsCdata`.
    decl_attribute_is_cdata: bool,
    /// `m_declAttributeIsId`.
    decl_attribute_is_id: bool,
    /// `m_dtd`.
    dtd: Dtd,
    /// `m_tagStack` (the innermost last).
    tag_stack: Vec<Tag>,
    /// `m_atts`, `m_attsSize`.
    atts: Vec<Attribute>,
    /// `m_nSpecifiedAtts`.
    n_specified_atts: i32,
    /// `m_idAttIndex`.
    id_att_index: i32,
    /// `m_position`.
    position: Position,
    /// `m_groupConnector`, `m_groupSize`.
    group_connector: Vec<u8>,
    /// `m_parsingStatus.parsing`.
    parsing: ParsingStatus,
    /// `m_parsingStatus.finalBuffer`.
    final_buffer: bool,
    /// `m_accounting`.
    accounting: Accounting,
    /// `m_reenter`.
    reenter: bool,
    /// `g_bytesScanned` (a global of expat's test builds, `XML_TESTING`): the bytes the
    /// processor was run on. Only the tests read it.
    bytes_scanned: u32,
    /// `m_alloc_tracker`.
    alloc_tracker: MallocTracker,
    /// The DTD's bytes, as one counted block a reset frees.
    dtd_block: Option<Block>,
    /// `m_atts`'s block, and `m_groupConnector`'s once allocated.
    atts_block: Option<Block>,
    group_block: Option<Block>,
    /// The most tags, open entities and attribute value bytes so far: expat keeps their
    /// memory (free lists, the pool's blocks) for reuse, so only growth past them is charged.
    tags_high_water: usize,
    open_entities_high_water: usize,
    temp_pool_high_water: usize,
}

impl<'a, E> Parser<'a, E> {
    /// A parser with expat's defaults, reparse deferral included.
    ///
    /// Port of `XML_ParserCreate(NULL)` (xmlparse.c:1008-1011), `parserCreate`
    /// (xmlparse.c:1333-1523) and `parserInit` (xmlparse.c:1525-1614).
    pub fn new() -> Parser<'a, E> {
        Parser::with_deferral_default(true)
    }

    /// A parser made while `g_reparseDeferralEnabledDefault` is `deferral`: the switch
    /// expat's own tests use.
    pub fn with_deferral_default(deferral: bool) -> Parser<'a, E> {
        let mut parser = Parser {
            start_element_handler: None,
            end_element_handler: None,
            character_data_handler: None,
            buffer: None,
            buffer_ptr: 0,
            buffer_end: 0,
            parse_end_byte_index: 0,
            parse_end_ptr: 0,
            partial_token_bytes_before: 0,
            reparse_deferral_enabled: deferral,
            reparse_deferral_default: deferral,
            last_buffer_request_size: 0,
            encoding: EncState::Init,
            init_encoding: InitEncoding::new(None).expect("no name is NO_ENC"),
            internal_encoding: xml_get_utf8_internal_encoding(),
            prolog_state: PrologState::new(),
            processor: Processor::PrologInit,
            error_code: XmlError::None,
            event_ptr: None,
            event_end_ptr: None,
            position_ptr: None,
            open_internal_entities: Vec::new(),
            open_attribute_entities: Vec::new(),
            tag_level: 0,
            decl_entity: None,
            doctype_sysid: false,
            decl_element_type: None,
            decl_attribute_id: None,
            decl_attribute_is_cdata: false,
            decl_attribute_is_id: false,
            dtd: Dtd::new(),
            tag_stack: Vec::new(),
            atts: vec![Attribute::default(); INIT_ATTS_SIZE],
            n_specified_atts: 0,
            id_att_index: 0,
            position: Position::default(),
            group_connector: Vec::new(),
            parsing: ParsingStatus::Initialized,
            final_buffer: false,
            accounting: Accounting {
                count_bytes_direct: 0,
                count_bytes_indirect: 0,
                maximum_amplification_factor: MAXIMUM_AMPLIFICATION_DEFAULT,
                activation_threshold_bytes: ACTIVATION_THRESHOLD_DEFAULT,
            },
            reenter: false,
            bytes_scanned: 0,
            alloc_tracker: MallocTracker {
                bytes_allocated: 0,
                maximum_amplification_factor: ALLOC_TRACKER_MAXIMUM_AMPLIFICATION_DEFAULT,
                activation_threshold_bytes: ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT,
            },
            dtd_block: None,
            atts_block: None,
            group_block: None,
            tags_high_water: 0,
            open_entities_high_water: 0,
            temp_pool_high_water: 0,
        };
        // Record XML_ParserStruct allocation we did a few lines up before (parserCreate,
        // xmlparse.c:1410-1414), then m_atts, m_dataBuf and the DTD (1436-1466).
        parser.alloc_tracker.bytes_allocated =
            (size_of::<usize>() + size_of::<Parser<'a, E>>()) as u64;
        // With nothing parsed, the activation threshold can't be reached.
        let _ = parser.expat_malloc(INIT_DATA_BUF_SIZE);
        parser.atts_block = parser.expat_malloc(INIT_ATTS_SIZE * size_of::<Attribute>());
        let _ = parser.expat_malloc(size_of::<Dtd>());
        parser
    }

    /// Clears the parser's state and handlers for a new document, keeping its buffer.
    ///
    /// Port of `XML_ParserReset(parser, NULL)` (xmlparse.c:1626-1683), `parserInit` and
    /// `dtdReset` (xmlparse.c:7506-7545). The free lists it fills are memory only.
    pub fn reset(&mut self) {
        let mut fresh = Parser::with_deferral_default(self.reparse_deferral_default);
        // parserInit points m_bufferPtr and m_bufferEnd at m_buffer, which it keeps; m_atts
        // and m_groupConnector keep their sizes.
        fresh.buffer = self.buffer.take();
        fresh.atts = std::mem::take(&mut self.atts);
        fresh.group_connector = std::mem::take(&mut self.group_connector);
        fresh.id_att_index = self.id_att_index;
        fresh.final_buffer = self.final_buffer;
        fresh.bytes_scanned = self.bytes_scanned;
        // parserInit leaves m_alloc_tracker alone; dtdReset frees the DTD.
        if let Some(block) = self.dtd_block.take() {
            self.expat_free(block);
        }
        fresh.alloc_tracker = self.alloc_tracker;
        fresh.atts_block = self.atts_block.take();
        fresh.group_block = self.group_block.take();
        fresh.tags_high_water = self.tags_high_water;
        fresh.open_entities_high_water = self.open_entities_high_water;
        fresh.temp_pool_high_water = self.temp_pool_high_water;
        *self = fresh;
    }

    /// Port of `XML_SetElementHandler` (xmlparse.c:2089-2096).
    pub fn set_element_handler(
        &mut self,
        start: Option<StartElementHandler<'a, E>>,
        end: Option<EndElementHandler<'a, E>>,
    ) {
        self.start_element_handler = start;
        self.end_element_handler = end;
    }

    /// Port of `XML_SetStartElementHandler` (xmlparse.c:2098-2102).
    pub fn set_start_element_handler(&mut self, start: Option<StartElementHandler<'a, E>>) {
        self.start_element_handler = start;
    }

    /// Port of `XML_SetEndElementHandler` (xmlparse.c:2104-2108).
    pub fn set_end_element_handler(&mut self, end: Option<EndElementHandler<'a, E>>) {
        self.end_element_handler = end;
    }

    /// Port of `XML_SetCharacterDataHandler` (xmlparse.c:2110-2115).
    pub fn set_character_data_handler(&mut self, handler: Option<CharacterDataHandler<'a, E>>) {
        self.character_data_handler = handler;
    }

    /// Port of `XML_SetReparseDeferralEnabled` (xmlparse.c:3097-3104): `enabled` is an
    /// `XML_Bool`, and only `XML_FALSE` (0) and `XML_TRUE` (1) are accepted.
    pub fn set_reparse_deferral_enabled(&mut self, enabled: u8) -> bool {
        if enabled == 1 || enabled == 0 {
            self.reparse_deferral_enabled = enabled == 1;
            return true;
        }
        false
    }

    /// Port of `XML_SetAllocTrackerMaximumAmplification` (xmlparse.c:3073-3084) for a parser
    /// without parent.
    pub fn set_alloc_tracker_maximum_amplification(&mut self, factor: f32) -> bool {
        if factor.is_nan() || factor < 1.0f32 {
            return false;
        }
        self.alloc_tracker.maximum_amplification_factor = factor;
        true
    }

    /// Port of `XML_SetAllocTrackerActivationThreshold` (xmlparse.c:3086-3094) for a parser
    /// without parent.
    pub fn set_alloc_tracker_activation_threshold(&mut self, bytes: u64) -> bool {
        self.alloc_tracker.activation_threshold_bytes = bytes;
        true
    }

    /// Whether the tracker allows `increase` more bytes.
    ///
    /// Port of `expat_heap_increase_tolerable` (xmlparse.c:812-844), without the report.
    fn expat_heap_increase_tolerable(&self, increase: u64) -> bool {
        // Detect integer overflow
        if u64::MAX - self.alloc_tracker.bytes_allocated < increase {
            return false;
        }
        let new_total = self.alloc_tracker.bytes_allocated + increase;
        if new_total >= self.alloc_tracker.activation_threshold_bytes {
            // NOTE: This can be +infinity when dividing by zero but not -nan
            let amplification = new_total as f32 / self.accounting.count_bytes_direct as f32;
            if amplification > self.alloc_tracker.maximum_amplification_factor {
                return false;
            }
        }
        true
    }

    /// Counts a block of `size` bytes, or refuses it (`NULL`, out of memory).
    ///
    /// Port of `expat_malloc` (xmlparse.c:846-898): the tracker's accounting, without the
    /// allocation itself, which the port's structures make.
    fn expat_malloc(&mut self, size: usize) -> Option<Block> {
        // Detect integer overflow
        if usize::MAX - size < size_of::<usize>() {
            return None;
        }
        let bytes_to_allocate = (size_of::<usize>() + size) as u64;
        if u64::MAX - self.alloc_tracker.bytes_allocated < bytes_to_allocate {
            return None; // i.e. signal integer overflow as out-of-memory
        }
        if !self.expat_heap_increase_tolerable(bytes_to_allocate) {
            return None; // i.e. signal violation as out-of-memory
        }
        // Update accounting
        self.alloc_tracker.bytes_allocated += bytes_to_allocate;
        Some(Block { size })
    }

    /// Port of `expat_free` (xmlparse.c:900-933): the tracker's accounting.
    fn expat_free(&mut self, block: Block) {
        self.alloc_tracker.bytes_allocated -= (size_of::<usize>() + block.size) as u64;
    }

    /// Resizes a counted block to `size` bytes; `false`, the block unchanged, when the tracker
    /// refuses the increase.
    ///
    /// Port of `expat_realloc` (xmlparse.c:935-1005) for a block (`ptr` not null) and a size
    /// not 0: the tracker's accounting.
    fn expat_realloc(&mut self, block: &mut Block, size: usize) -> bool {
        let prev_size = block.size;
        // Classify upcoming change
        let is_increase = size > prev_size;
        let abs_diff = size.abs_diff(prev_size) as u64;
        // Ask for permission from accounting
        if is_increase && !self.expat_heap_increase_tolerable(abs_diff) {
            return false; // i.e. signal violation as out-of-memory
        }
        if is_increase {
            self.alloc_tracker.bytes_allocated += abs_diff;
        } else {
            self.alloc_tracker.bytes_allocated -= abs_diff;
        }
        // Update in-block recorded size
        block.size = size;
        true
    }

    /// Charges a block of `size` bytes the parser keeps, or `Err(XML_ERROR_NO_MEMORY)` where
    /// expat's `MALLOC` would return null.
    fn charge(&mut self, size: usize) -> Result<(), XmlError> {
        match self.expat_malloc(size) {
            Some(_) => Ok(()),
            None => Err(XmlError::NoMemory),
        }
    }

    /// [`Parser::charge`] for the DTD, whose bytes a reset frees.
    fn charge_dtd(&mut self, size: usize) -> Result<(), XmlError> {
        let block = match self.dtd_block.take() {
            None => self.expat_malloc(size),
            Some(mut block) => {
                let new_size = block.size + size;
                let grown = self.expat_realloc(&mut block, new_size);
                self.dtd_block = Some(block);
                if !grown {
                    return Err(XmlError::NoMemory);
                }
                return Ok(());
            }
        };
        self.dtd_block = Some(block.ok_or(XmlError::NoMemory)?);
        Ok(())
    }

    /// Resizes one of the parser's counted blocks (`REALLOC`), or `Err(XML_ERROR_NO_MEMORY)`.
    fn charge_realloc(
        &mut self,
        which: fn(&mut Self) -> &mut Option<Block>,
        size: usize,
    ) -> Result<(), XmlError> {
        let ok = match which(self).take() {
            None => {
                let block = self.expat_malloc(size);
                let ok = block.is_some();
                *which(self) = block;
                ok
            }
            Some(mut block) => {
                let ok = self.expat_realloc(&mut block, size);
                *which(self) = Some(block);
                ok
            }
        };
        if ok { Ok(()) } else { Err(XmlError::NoMemory) }
    }

    /// Charges the growth of a structure expat keeps for reuse past its most so far.
    fn charge_growth(
        &mut self,
        high_water: usize,
        now: usize,
        unit: usize,
    ) -> Result<usize, XmlError> {
        if now > high_water {
            self.charge((now - high_water) * unit)?;
            return Ok(now);
        }
        Ok(high_water)
    }

    /// `lookup(parser, table, name, sizeof(ENTITY))` on the general or the parameter
    /// entities (xmlparse.c:7811-7898), the name in the DTD's pool: the entity `name`, made if
    /// missing (charged: the entry, its name and its table slot); and whether it was made.
    fn lookup_entity(&mut self, param: bool, name: &[u8]) -> Result<(usize, bool), XmlError> {
        let table = if param {
            &self.dtd.param_entities
        } else {
            &self.dtd.general_entities
        };
        if let Some(&id) = table.get(name) {
            return Ok((id, false));
        }
        self.charge_dtd(size_of::<Entity>() + name.len() + 1 + size_of::<usize>())?;
        Ok(self.dtd.lookup_entity(param, name))
    }

    /// The element type `name`, made if missing (charged as [`Parser::lookup_entity`]).
    fn lookup_element_type(&mut self, name: &[u8]) -> Result<usize, XmlError> {
        if let Some(&id) = self.dtd.element_types.get(name) {
            return Ok(id);
        }
        self.charge_dtd(size_of::<ElementType>() + name.len() + 1 + size_of::<usize>())?;
        Ok(self.dtd.lookup_element_type(name))
    }

    /// Port of `XML_GetErrorCode` (xmlparse.c:2729-2734).
    pub fn error_code(&self) -> XmlError {
        self.error_code
    }

    /// Port of `XML_GetSpecifiedAttributeCount` (xmlparse.c:2066-2071).
    pub fn specified_attribute_count(&self) -> i32 {
        self.n_specified_atts
    }

    /// Port of `XML_GetIdAttributeIndex` (xmlparse.c:2073-2079).
    pub fn id_attribute_index(&self) -> i32 {
        self.id_att_index
    }

    /// Port of `XML_GetCurrentByteIndex` (xmlparse.c:2736-2744).
    pub fn current_byte_index(&self) -> i64 {
        match self.event_ptr {
            Some(event) => {
                self.parse_end_byte_index as i64 - (self.parse_end_ptr as i64 - event as i64)
            }
            None => -1,
        }
    }

    /// Port of `XML_GetCurrentByteCount` (xmlparse.c:2746-2753).
    pub fn current_byte_count(&self) -> i32 {
        match (self.event_end_ptr, self.event_ptr) {
            (Some(end), Some(event)) => (end as i64 - event as i64) as i32,
            _ => 0,
        }
    }

    /// Port of `XML_GetCurrentLineNumber` (xmlparse.c:2775-2785).
    pub fn current_line_number(&mut self) -> u64 {
        self.update_position_to_event();
        self.position.line_number + 1
    }

    /// Port of `XML_GetCurrentColumnNumber` (xmlparse.c:2787-2797).
    pub fn current_column_number(&mut self) -> u64 {
        self.update_position_to_event();
        self.position.column_number
    }

    /// `if (m_eventPtr && m_eventPtr >= m_positionPtr) { XmlUpdatePosition(m_encoding,
    /// m_positionPtr, m_eventPtr, &m_position); m_positionPtr = m_eventPtr; }`.
    /// `m_positionPtr` is null only after `XML_GetBuffer` cleared `m_eventPtr` too.
    fn update_position_to_event(&mut self) {
        if let (Some(event), Some(from)) = (self.event_ptr, self.position_ptr)
            && event >= from
        {
            let buffer = self.buffer.take().unwrap_or_default();
            self.update_position(&buffer, from, event);
            self.buffer = Some(buffer);
            self.position_ptr = Some(event);
        }
    }

    /// `XmlUpdatePosition(parser->m_encoding, ...)`.
    fn update_position(&mut self, b: &[u8], ptr: usize, end: usize) {
        match self.encoding {
            EncState::Init => self
                .init_encoding
                .update_position(b, ptr, end, &mut self.position),
            EncState::Known(enc) => enc.update_position(b, ptr, end, &mut self.position),
        }
    }

    /// The parser's encoding once the first token has set it.
    fn known_encoding(&self) -> &'static Encoding {
        match self.encoding {
            EncState::Known(enc) => enc,
            EncState::Init => unreachable!("the encoding is known once a token has been read"),
        }
    }

    /// Parses `s`, the document's next bytes; `is_final` for its last.
    ///
    /// Port of `XML_Parse` (xmlparse.c:2324-2433) with `XML_CONTEXT_BYTES` 1024: the bytes go
    /// into the parser's buffer (`XML_GetBuffer`) and are parsed there (`XML_ParseBuffer`).
    pub fn parse(&mut self, s: &[u8], is_final: bool) -> Result<XmlStatus, ParseError<E>> {
        let Ok(len) = i32::try_from(s.len()) else {
            // `int len`: a C caller can't pass more.
            self.error_code = XmlError::InvalidArgument;
            return Ok(XmlStatus::Error);
        };
        match self.parsing {
            ParsingStatus::Finished => {
                self.error_code = XmlError::Finished;
                return Ok(XmlStatus::Error);
            }
            // startParsing: the hash salt only.
            ParsingStatus::Initialized | ParsingStatus::Parsing => {
                self.parsing = ParsingStatus::Parsing
            }
        }
        let Some(at) = self.get_buffer(len) else {
            return Ok(XmlStatus::Error);
        };
        if len > 0 {
            let buffer = self.buffer.as_mut().expect("XML_GetBuffer made a buffer");
            buffer[at..at + s.len()].copy_from_slice(s);
        }
        self.parse_buffer(len, is_final)
    }

    /// Room for `len` more bytes in the parser's buffer, at the returned position; `None` with
    /// the error code set when the buffer can't grow.
    ///
    /// Port of `XML_GetBuffer` (xmlparse.c:2506-2638) with `XML_CONTEXT_BYTES` 1024.
    pub fn get_buffer(&mut self, len: i32) -> Option<usize> {
        if len < 0 {
            self.error_code = XmlError::NoMemory;
            return None;
        }
        if self.parsing == ParsingStatus::Finished {
            self.error_code = XmlError::Finished;
            return None;
        }

        // whether or not the request succeeds, `len` seems to be the app's preferred buffer
        // fill size; remember it.
        self.last_buffer_request_size = len;
        let lim = self.buffer.as_ref().map_or(0, Vec::len);
        if len as usize > lim - self.buffer_end || self.buffer.is_none() {
            // Do not invoke signed arithmetic overflow:
            let needed_size =
                (len as u32).wrapping_add((self.buffer_end - self.buffer_ptr) as u32) as i32;
            if needed_size < 0 {
                self.error_code = XmlError::NoMemory;
                return None;
            }
            let keep = (self.buffer_ptr as i32).min(XML_CONTEXT_BYTES);
            // Detect and prevent integer overflow
            if keep > i32::MAX - needed_size {
                self.error_code = XmlError::NoMemory;
                return None;
            }
            let needed_size = needed_size + keep;
            let keep = keep as usize;
            if let Some(buffer) = self.buffer.as_mut()
                && needed_size as usize <= lim
            {
                if keep < self.buffer_ptr {
                    let offset = self.buffer_ptr - keep;
                    let count = self.buffer_end - self.buffer_ptr + keep;
                    buffer.copy_within(offset..offset + count, 0);
                    self.buffer_end -= offset;
                    self.buffer_ptr -= offset;
                }
            } else {
                let mut buffer_size = lim as i32;
                if buffer_size == 0 {
                    buffer_size = INIT_BUFFER_SIZE;
                }
                loop {
                    // Do not invoke signed arithmetic overflow:
                    buffer_size = 2u32.wrapping_mul(buffer_size as u32) as i32;
                    if !(buffer_size < needed_size && buffer_size > 0) {
                        break;
                    }
                }
                if buffer_size <= 0 {
                    self.error_code = XmlError::NoMemory;
                    return None;
                }
                let mut new_buf = vec![0u8; buffer_size as usize];
                match self.buffer.take() {
                    Some(old) => {
                        let count = self.buffer_end - self.buffer_ptr + keep;
                        let from = self.buffer_ptr - keep;
                        new_buf[..count].copy_from_slice(&old[from..from + count]);
                        self.buffer_end = count;
                        self.buffer_ptr = keep;
                    }
                    None => {
                        // This must be a brand new buffer with no data in it yet
                        self.buffer_end = 0;
                        self.buffer_ptr = 0;
                    }
                }
                self.buffer = Some(new_buf);
            }
            self.event_ptr = None;
            self.event_end_ptr = None;
            self.position_ptr = None;
        }
        Some(self.buffer_end)
    }

    /// Parses the `len` bytes put in the buffer at [`Parser::get_buffer`]'s position.
    ///
    /// Port of `XML_ParseBuffer` (xmlparse.c:2435-2504).
    pub fn parse_buffer(&mut self, len: i32, is_final: bool) -> Result<XmlStatus, ParseError<E>> {
        if len < 0 {
            self.error_code = XmlError::InvalidArgument;
            return Ok(XmlStatus::Error);
        }
        match self.parsing {
            ParsingStatus::Finished => {
                self.error_code = XmlError::Finished;
                return Ok(XmlStatus::Error);
            }
            ParsingStatus::Initialized => {
                // Has someone called XML_GetBuffer successfully before?
                if self.buffer.is_none() {
                    self.error_code = XmlError::NoBuffer;
                    return Ok(XmlStatus::Error);
                }
                self.parsing = ParsingStatus::Parsing;
            }
            ParsingStatus::Parsing => {}
        }

        let start = self.buffer_ptr;
        self.position_ptr = Some(start);
        self.buffer_end += len as usize;
        self.parse_end_ptr = self.buffer_end;
        self.parse_end_byte_index += len as u64;
        self.final_buffer = is_final;

        let buffer = self.buffer.take().expect("a buffer");
        let mut end_ptr = start;
        let result = self.call_processor(&buffer, start, self.parse_end_ptr, &mut end_ptr);
        self.buffer = Some(buffer);
        self.buffer_ptr = end_ptr;
        self.error_code = result.map_err(ParseError::Handler)?;

        if self.error_code != XmlError::None {
            self.event_end_ptr = self.event_ptr;
            self.processor = Processor::Error;
            return Ok(XmlStatus::Error);
        }
        if is_final {
            self.parsing = ParsingStatus::Finished;
            return Ok(XmlStatus::Ok);
        }

        let buffer = self.buffer.take().expect("a buffer");
        let from = self.position_ptr.expect("set above");
        self.update_position(&buffer, from, self.buffer_ptr);
        self.buffer = Some(buffer);
        self.position_ptr = Some(self.buffer_ptr);
        Ok(XmlStatus::Ok)
    }

    /// Runs the processor on `b[start..end]` (the parse buffer), again while an internal
    /// entity asks for it; or, with reparse deferral, not yet, if the last call left a partial
    /// token and too little has arrived since.
    ///
    /// Port of `callProcessor` (xmlparse.c:1247-1310).
    fn call_processor(
        &mut self,
        b: &[u8],
        start: usize,
        end: usize,
        end_ptr: &mut usize,
    ) -> HResult<E> {
        let have_now = end - start;

        if self.reparse_deferral_enabled && !self.final_buffer {
            // Heuristic: don't try to parse a partial token again until the amount of
            // available data has increased significantly.
            let had_before = self.partial_token_bytes_before;
            // ...but *do* try anyway if we're close to causing a reallocation.
            let mut available_buffer = self.buffer_ptr;
            available_buffer -= available_buffer.min(XML_CONTEXT_BYTES as usize);
            available_buffer += b.len() - self.buffer_end;
            // m_lastBufferRequestSize is never assigned a value < 0, so the cast is ok
            let enough = (have_now >= 2 * had_before)
                || (self.last_buffer_request_size as usize > available_buffer);

            if !enough {
                *end_ptr = start; // callers may expect this to be set
                return Ok(XmlError::None);
            }
        }
        self.bytes_scanned = self.bytes_scanned.wrapping_add(have_now as u32);
        // Run in a loop to eliminate dangerous recursion depths
        let mut ret;
        *end_ptr = start;
        loop {
            // Use endPtr as the new start in each iteration, since it will be set to the next
            // start point by m_processor.
            let s = *end_ptr;
            ret = self.run_processor(b, s, end, end_ptr)?;

            // Make parsing status (and in particular XML_SUSPENDED) take precedence over
            // re-enter flag when they disagree
            if self.parsing != ParsingStatus::Parsing {
                self.reenter = false;
            }

            if !self.reenter {
                break;
            }

            self.reenter = false;
            if ret != XmlError::None {
                return Ok(ret);
            }
        }

        if ret == XmlError::None {
            // if we consumed nothing, remember what we had on this parse attempt.
            if *end_ptr == start {
                self.partial_token_bytes_before = have_now;
            } else {
                self.partial_token_bytes_before = 0;
            }
        }
        Ok(ret)
    }

    /// `parser->m_processor(parser, s, end, nextPtr)`.
    fn run_processor(
        &mut self,
        b: &[u8],
        s: usize,
        end: usize,
        next_ptr: &mut usize,
    ) -> HResult<E> {
        match self.processor {
            Processor::PrologInit => self.prolog_init_processor(b, s, end, next_ptr),
            Processor::Prolog => self.prolog_processor(b, s, end, next_ptr),
            Processor::Content => self.content_processor(b, s, end, next_ptr),
            Processor::CdataSection => self.cdata_section_processor(b, s, end, next_ptr),
            Processor::Epilog => self.epilog_processor(b, s, end, next_ptr),
            Processor::InternalEntity => self.internal_entity_processor(),
            // errorProcessor (xmlparse.c:6467-6474)
            Processor::Error => Ok(self.error_code),
        }
    }

    // ---- events ----

    /// `*eventPP = p`.
    fn set_event_ptr(&mut self, src: Src, p: usize) {
        match src {
            Src::Main => self.event_ptr = Some(p),
            Src::Entity(i) => self.open_internal_entities[i].internal_event_ptr = Some(p),
        }
    }

    /// `*eventEndPP = p`.
    fn set_event_end_ptr(&mut self, src: Src, p: usize) {
        match src {
            Src::Main => self.event_end_ptr = Some(p),
            Src::Entity(i) => self.open_internal_entities[i].internal_event_end_ptr = Some(p),
        }
    }

    /// `*eventEndPP`.
    fn event_end_ptr_of(&self, src: Src) -> Option<usize> {
        match src {
            Src::Main => self.event_end_ptr,
            Src::Entity(i) => self.open_internal_entities[i].internal_event_end_ptr,
        }
    }

    /// The character data handler with `data`, if there is one.
    fn report_char_data(&mut self, data: &[u8]) -> Result<(), E> {
        match self.character_data_handler.as_mut() {
            Some(handler) => handler(data),
            None => Ok(()),
        }
    }

    // ---- accounting ----

    /// The amplification so far.
    ///
    /// Port of `accountingGetCurrentAmplification` (xmlparse.c:8443-8459).
    fn accounting_get_current_amplification(&self) -> f32 {
        //                                         1.........1.........12 => 22
        const LEN_OF_SHORTEST_INCLUDE: u64 = b"<!ENTITY a SYSTEM 'b'>".len() as u64;
        let direct = self.accounting.count_bytes_direct;
        let indirect = self.accounting.count_bytes_indirect;
        let count_bytes_output = direct.wrapping_add(indirect);
        if direct != 0 {
            count_bytes_output as f32 / direct as f32
        } else {
            LEN_OF_SHORTEST_INCLUDE.wrapping_add(indirect) as f32 / LEN_OF_SHORTEST_INCLUDE as f32
        }
    }

    /// Counts the `bytes_more` bytes of the token `tok`, and whether the amplification is
    /// still tolerated.
    ///
    /// Port of `accountingDiffTolerated` (xmlparse.c:8522-8575) for a parser without parent:
    /// the debug report is left out.
    fn accounting_diff_tolerated(&mut self, tok: i32, bytes_more: usize, account: Account) -> bool {
        // Note: We need to check the token type *first* to be sure that we can even access
        // variable <after>, safely.
        if let XML_TOK_INVALID | XML_TOK_PARTIAL | XML_TOK_PARTIAL_CHAR | XML_TOK_NONE = tok {
            return true;
        }

        let addition_target = match account {
            // because these bytes have been accounted for, already
            Account::None => return true,
            Account::Direct => &mut self.accounting.count_bytes_direct,
            Account::EntityExpansion => &mut self.accounting.count_bytes_indirect,
        };

        // Detect and avoid integer overflow
        let bytes_more = bytes_more as u64;
        if *addition_target > u64::MAX - bytes_more {
            return false;
        }
        *addition_target += bytes_more;

        let count_bytes_output = self
            .accounting
            .count_bytes_direct
            .wrapping_add(self.accounting.count_bytes_indirect);
        let amplification_factor = self.accounting_get_current_amplification();
        (count_bytes_output < self.accounting.activation_threshold_bytes)
            || (amplification_factor <= self.accounting.maximum_amplification_factor)
    }

    // ---- the prolog ----

    /// Port of `prologInitProcessor` (xmlparse.c:4967-4977) and `initializeEncoding`
    /// (xmlparse.c:4811-4839) without a protocol encoding.
    fn prolog_init_processor(
        &mut self,
        b: &[u8],
        s: usize,
        end: usize,
        next_ptr: &mut usize,
    ) -> HResult<E> {
        self.init_encoding = InitEncoding::new(None).expect("no name is NO_ENC");
        self.encoding = EncState::Init;
        self.processor = Processor::Prolog;
        self.prolog_processor(b, s, end, next_ptr)
    }

    /// Port of `prologProcessor` (xmlparse.c:5167-5175).
    ///
    /// Until its first token sets it, the parser's encoding is the initial one, whose
    /// scanners give only `XML_TOK_NONE` and `XML_TOK_PARTIAL` without setting it; `doProlog`
    /// reads those before it reads the encoding, and so does the port here.
    fn prolog_processor(
        &mut self,
        b: &[u8],
        s: usize,
        end: usize,
        next_ptr: &mut usize,
    ) -> HResult<E> {
        let mut next = s;
        let tok = match self.encoding {
            EncState::Known(enc) => enc.prolog_tok(b, s, end, &mut next),
            EncState::Init => {
                let (tok, enc) = self
                    .init_encoding
                    .scan(XML_PROLOG_STATE, b, s, end, &mut next);
                match enc {
                    Some(enc) => self.encoding = EncState::Known(enc),
                    None => {
                        // doProlog's first steps (xmlparse.c:5225-5241).
                        self.event_ptr = Some(s);
                        self.event_end_ptr = Some(next);
                        if !self.final_buffer {
                            *next_ptr = s;
                            return Ok(XmlError::None);
                        }
                        return Ok(match tok {
                            XML_TOK_PARTIAL => XmlError::UnclosedToken,
                            XML_TOK_NONE => XmlError::NoElements,
                            _ => unreachable!("initScan sets the encoding for other tokens"),
                        });
                    }
                }
                tok
            }
        };
        let enc = self.known_encoding();
        let have_more = !self.final_buffer;
        self.do_prolog(
            enc,
            b,
            s,
            end,
            tok,
            next,
            next_ptr,
            have_more,
            Account::Direct,
        )
    }

    /// Port of `doProlog` (xmlparse.c:5177-6248) for the document entity, whose prolog is
    /// always read from the parse buffer in the parser's encoding.
    #[allow(clippy::too_many_arguments)]
    fn do_prolog(
        &mut self,
        mut enc: &'static Encoding,
        b: &[u8],
        mut s: usize,
        end: usize,
        mut tok: i32,
        mut next: usize,
        next_ptr: &mut usize,
        have_more: bool,
        account: Account,
    ) -> HResult<E> {
        loop {
            self.event_ptr = Some(s);
            self.event_end_ptr = Some(next);
            if tok <= 0 {
                if have_more && tok != XML_TOK_INVALID {
                    *next_ptr = s;
                    return Ok(XmlError::None);
                }
                match tok {
                    XML_TOK_INVALID => {
                        self.event_ptr = Some(next);
                        return Ok(XmlError::InvalidToken);
                    }
                    XML_TOK_PARTIAL => return Ok(XmlError::UnclosedToken),
                    XML_TOK_PARTIAL_CHAR => return Ok(XmlError::PartialChar),
                    t if t == -XML_TOK_PROLOG_S => tok = -tok,
                    // The checks of internal parameter entities don't apply to the document.
                    XML_TOK_NONE => return Ok(XmlError::NoElements),
                    _ => {
                        tok = -tok;
                        next = end;
                    }
                }
            }
            let role = self.prolog_state.token_role(tok, b, s, next, enc);
            match role {
                // bytes accounted in contentProcessor, processXmlDecl
                XML_ROLE_INSTANCE_START | XML_ROLE_XML_DECL => {}
                _ => {
                    if !self.accounting_diff_tolerated(tok, next - s, account) {
                        return Ok(XmlError::AmplificationLimitBreach);
                    }
                }
            }
            match role {
                XML_ROLE_XML_DECL => {
                    let result = self.process_xml_decl(b, s, next);
                    if result != XmlError::None {
                        return Ok(result);
                    }
                    enc = self.known_encoding();
                }
                XML_ROLE_DOCTYPE_NAME => {
                    self.doctype_sysid = false; // always initialize to NULL
                }
                XML_ROLE_DOCTYPE_PUBLIC_ID | XML_ROLE_ENTITY_PUBLIC_ID => {
                    if role == XML_ROLE_DOCTYPE_PUBLIC_ID {
                        self.decl_entity = match self.lookup_entity(true, b"#") {
                            Ok((id, _)) => Some(id),
                            Err(result) => return Ok(result),
                        };
                        self.dtd.has_param_entity_refs = true;
                    }
                    let mut bad = 0usize;
                    if !enc.is_public_id(b, s, next, &mut bad) {
                        self.event_ptr = Some(bad);
                        return Ok(XmlError::Publicid);
                    }
                    // The public id itself is read only by handlers.
                }
                XML_ROLE_DOCTYPE_CLOSE => {
                    // m_doctypeSysid is non-NULL after a system id, even without a doctype
                    // handler, indicating an external subset (m_useForeignDTD is false).
                    if self.doctype_sysid {
                        self.dtd.has_param_entity_refs = true;
                        // No parameter entity parsing: the external subset is not read.
                    }
                }
                XML_ROLE_INSTANCE_START => {
                    self.processor = Processor::Content;
                    return self.content_processor(b, s, end, next_ptr);
                }
                XML_ROLE_ATTLIST_ELEMENT_NAME => {
                    let name = pool_store_string(enc, b, s, next);
                    self.decl_element_type = match self.lookup_element_type(&name) {
                        Ok(id) => Some(id),
                        Err(result) => return Ok(result),
                    };
                }
                XML_ROLE_ATTRIBUTE_NAME => {
                    self.decl_attribute_id = match self.get_attribute_id(enc, b, s, next) {
                        Ok(id) => Some(id),
                        Err(result) => return Ok(result),
                    };
                    self.decl_attribute_is_cdata = false;
                    self.decl_attribute_is_id = false;
                }
                XML_ROLE_ATTRIBUTE_TYPE_CDATA => self.decl_attribute_is_cdata = true,
                XML_ROLE_ATTRIBUTE_TYPE_ID => self.decl_attribute_is_id = true,
                XML_ROLE_IMPLIED_ATTRIBUTE_VALUE | XML_ROLE_REQUIRED_ATTRIBUTE_VALUE => {
                    if self.dtd.keep_processing
                        && let Err(result) = self.define_attribute(
                            self.decl_attribute_is_cdata,
                            self.decl_attribute_is_id,
                            None,
                        )
                    {
                        return Ok(result);
                    }
                }
                XML_ROLE_DEFAULT_ATTRIBUTE_VALUE | XML_ROLE_FIXED_ATTRIBUTE_VALUE => {
                    if self.dtd.keep_processing {
                        let mbpc = enc.min_bytes_per_char;
                        let att_val = match self.store_attribute_value(
                            true,
                            enc,
                            self.decl_attribute_is_cdata,
                            b,
                            s + mbpc,
                            next - mbpc,
                            true,
                            Account::None,
                        ) {
                            Ok(value) => value,
                            Err(result) => return Ok(result),
                        };
                        // ID attributes aren't allowed to have a default
                        if let Err(result) = self.charge_dtd(att_val.len() + 1).and_then(|()| {
                            self.define_attribute(
                                self.decl_attribute_is_cdata,
                                false,
                                Some(att_val.into()),
                            )
                        }) {
                            return Ok(result);
                        }
                    }
                }
                XML_ROLE_ENTITY_VALUE => {
                    if self.dtd.keep_processing {
                        let mbpc = enc.min_bytes_per_char;
                        // This will store the given replacement text in the entity.
                        let (result, text) =
                            self.call_store_entity_value(enc, b, s + mbpc, next - mbpc);
                        // The entity value pool's bytes.
                        if let Err(result) = self.charge_dtd(text.len()) {
                            return Ok(result);
                        }
                        if let Some(id) = self.decl_entity {
                            self.dtd.entities[id].text = Some(text.into());
                        }
                        if result != XmlError::None {
                            return Ok(result);
                        }
                    }
                }
                XML_ROLE_DOCTYPE_SYSTEM_ID => {
                    self.dtd.has_param_entity_refs = true;
                    // use externalSubsetName to make m_doctypeSysid non-NULL for the case
                    // where no m_startDoctypeDeclHandler is set
                    self.doctype_sysid = true;
                    if self.decl_entity.is_none() {
                        self.decl_entity = match self.lookup_entity(true, b"#") {
                            Ok((id, _)) => Some(id),
                            Err(result) => return Ok(result),
                        };
                    }
                    // Falls through to XML_ROLE_ENTITY_SYSTEM_ID: the system id itself is read
                    // only by handlers.
                }
                XML_ROLE_ENTITY_NOTATION_NAME => {
                    if self.dtd.keep_processing
                        && let Some(id) = self.decl_entity
                    {
                        self.dtd.entities[id].has_notation = true;
                    }
                }
                XML_ROLE_GENERAL_ENTITY_NAME => {
                    if enc.predefined_entity_name(b, s, next) != 0 {
                        self.decl_entity = None;
                    } else if self.dtd.keep_processing {
                        let name = pool_store_string(enc, b, s, next);
                        self.decl_entity = match self.declare_entity(false, &name) {
                            Ok(id) => id,
                            Err(result) => return Ok(result),
                        };
                    } else {
                        self.decl_entity = None;
                    }
                }
                XML_ROLE_PARAM_ENTITY_NAME => {
                    if self.dtd.keep_processing {
                        let name = pool_store_string(enc, b, s, next);
                        self.decl_entity = match self.declare_entity(true, &name) {
                            Ok(id) => id,
                            Err(result) => return Ok(result),
                        };
                    } else {
                        self.decl_entity = None;
                    }
                }
                XML_ROLE_ERROR => {
                    return Ok(match tok {
                        // PE references in internal subset are not allowed within
                        // declarations.
                        XML_TOK_PARAM_ENTITY_REF => XmlError::ParamEntityRef,
                        XML_TOK_XML_DECL => XmlError::MisplacedXmlPi,
                        _ => XmlError::Syntax,
                    });
                }
                XML_ROLE_GROUP_OPEN => {
                    let level = self.prolog_state.level as usize;
                    if level >= self.group_connector.len() {
                        let size = if self.group_connector.is_empty() {
                            32
                        } else {
                            self.group_connector.len() * 2
                        };
                        if let Err(result) = self.charge_realloc(|p| &mut p.group_block, size) {
                            return Ok(result);
                        }
                        self.group_connector.resize(size, 0);
                    }
                    self.group_connector[level] = 0;
                }
                XML_ROLE_GROUP_SEQUENCE => {
                    let level = self.prolog_state.level as usize;
                    if self.group_connector[level] == b'|' {
                        return Ok(XmlError::Syntax);
                    }
                    self.group_connector[level] = b',';
                }
                XML_ROLE_GROUP_CHOICE => {
                    let level = self.prolog_state.level as usize;
                    if self.group_connector[level] == b',' {
                        return Ok(XmlError::Syntax);
                    }
                    self.group_connector[level] = b'|';
                }
                XML_ROLE_PARAM_ENTITY_REF | XML_ROLE_INNER_PARAM_ENTITY_REF => {
                    self.dtd.has_param_entity_refs = true;
                    // m_paramEntityParsing is XML_PARAM_ENTITY_PARSING_NEVER.
                    self.dtd.keep_processing = self.dtd.standalone;
                }
                // The other roles act only through handlers.
                _ => {}
            }

            // The parser is never suspended or finished, and internal entities are opened
            // only in content: the loop goes on.
            s = next;
            tok = enc.prolog_tok(b, s, end, &mut next);
        }
    }

    /// `XML_ROLE_GENERAL_ENTITY_NAME` and `XML_ROLE_PARAM_ENTITY_NAME` with `keepProcessing`
    /// (xmlparse.c:5743-5808): the entity `name` if this declares it first, `None` if it was
    /// declared before.
    fn declare_entity(&mut self, is_param: bool, name: &[u8]) -> Result<Option<usize>, XmlError> {
        let (id, made) = self.lookup_entity(is_param, name)?;
        if !made {
            return Ok(None);
        }
        // if we have a parent parser or are reading an internal parameter entity, then the
        // entity declaration is not considered "internal"
        self.dtd.entities[id].is_internal = self.open_internal_entities.is_empty();
        Ok(Some(id))
    }

    /// Port of `processXmlDecl(parser, 0, s, next)` (xmlparse.c:4841-4931) for the document
    /// entity, without a protocol encoding or handlers.
    fn process_xml_decl(&mut self, b: &[u8], s: usize, next: usize) -> XmlError {
        if !self.accounting_diff_tolerated(XML_TOK_XML_DECL, next - s, Account::Direct) {
            return XmlError::AmplificationLimitBreach;
        }

        let encoding = self.known_encoding();
        let decl = match xml_parse_xml_decl(false, encoding, b, s, next) {
            Ok(decl) => decl,
            Err(bad) => {
                self.event_ptr = Some(bad);
                return XmlError::XmlDecl;
            }
        };
        if decl.standalone == 1 {
            self.dtd.standalone = true;
        }
        match decl.encoding {
            Some(Some(new_encoding)) => {
                // Check that the specified encoding does not conflict with what the parser
                // has already deduced.  Do we have the same number of bytes in the smallest
                // representation of a character?  If this is UTF-16, is it the same
                // endianness?
                if new_encoding.min_bytes_per_char != encoding.min_bytes_per_char
                    || (new_encoding.min_bytes_per_char == 2 && new_encoding != encoding)
                {
                    self.event_ptr = decl.encoding_name;
                    return XmlError::IncorrectEncoding;
                }
                self.encoding = EncState::Known(new_encoding);
            }
            Some(None) => {
                // handleUnknownEncoding without a handler (xmlparse.c:4933-4965).
                self.event_ptr = decl.encoding_name;
                return XmlError::UnknownEncoding;
            }
            None => {}
        }
        XmlError::None
    }

    /// The attribute name `b[start..end]`, made if missing.
    ///
    /// Port of `getAttributeId` (xmlparse.c:7230-7291) without namespace processing.
    fn get_attribute_id(
        &mut self,
        enc: &Encoding,
        b: &[u8],
        start: usize,
        end: usize,
    ) -> Result<usize, XmlError> {
        let name = pool_store_string(enc, b, start, end);
        if let Some(&id) = self.dtd.attribute_ids.get(&name) {
            return Ok(id);
        }
        // The entry, its name with the byte before it, and its table slot.
        self.charge_dtd(size_of::<AttributeId>() + name.len() + 2 + size_of::<usize>())?;
        let id = self.dtd.att_ids.len();
        self.dtd.att_ids.push(AttributeId {
            name: name.as_slice().into(),
            specified: false,
            maybe_tokenized: false,
        });
        self.dtd.attribute_ids.insert(name, id);
        Ok(id)
    }

    /// Declares the current attribute of the current element type.
    ///
    /// Port of `defineAttribute(m_declElementType, m_declAttributeId, ...)`
    /// (xmlparse.c:7140-7199).
    fn define_attribute(
        &mut self,
        is_cdata: bool,
        is_id: bool,
        value: Option<Rc<[u8]>>,
    ) -> Result<(), XmlError> {
        let att_id = self
            .decl_attribute_id
            .expect("an attribute list names its attribute");
        let ty = self
            .decl_element_type
            .expect("an attribute list names its element");
        let element = &mut self.dtd.elements[ty];
        if value.is_some() || is_id {
            // The handling of default attributes gets messed up if we have a default which
            // duplicates a non-default.
            if element.default_atts.iter().any(|da| da.id == att_id) {
                return Ok(());
            }
            if is_id && element.id_att.is_none() {
                element.id_att = Some(att_id);
            }
        }
        // defaultAtts: 8 at first, then twice as many.
        let len = element.default_atts.len();
        if len == 0 || (len >= 8 && len.is_power_of_two()) {
            let grow = if len == 0 { 8 } else { len };
            self.charge_dtd(grow * size_of::<DefaultAttribute>())?;
        }
        let element = &mut self.dtd.elements[ty];
        element.default_atts.push(DefaultAttribute {
            id: att_id,
            is_cdata,
            value,
        });
        if !is_cdata {
            self.dtd.att_ids[att_id].maybe_tokenized = true;
        }
        Ok(())
    }

    /// The replacement text of an entity value, `b[ptr..end]` in `enc`, and how its reading
    /// ended.
    ///
    /// Port of `callStoreEntityValue` (xmlparse.c:6928-6999) for the document's internal
    /// subset, where no value entity is ever opened: one `storeEntityValue` reads it all.
    fn call_store_entity_value(
        &mut self,
        enc: &'static Encoding,
        b: &[u8],
        ptr: usize,
        end: usize,
    ) -> (XmlError, Vec<u8>) {
        let mut text = Vec::new();
        let result = self.store_entity_value(enc, b, ptr, end, &mut text);
        (result, text)
    }

    /// Port of `storeEntityValue` (xmlparse.c:6747-6926) with `XML_ACCOUNT_NONE`, on the
    /// document entity (`enc == parser->m_encoding`, not a parameter entity).
    fn store_entity_value(
        &mut self,
        enc: &'static Encoding,
        b: &[u8],
        mut entity_text_ptr: usize,
        entity_text_end: usize,
        pool: &mut Vec<u8>,
    ) -> XmlError {
        let old_in_entity_value = self.prolog_state.in_entity_value;
        self.prolog_state.in_entity_value = true;
        let mbpc = enc.min_bytes_per_char;
        let result = loop {
            // XmlEntityValueTok doesn't always set the last arg
            let mut next = entity_text_ptr;
            let tok = enc.entity_value_tok(b, entity_text_ptr, entity_text_end, &mut next);

            if !self.accounting_diff_tolerated(tok, next - entity_text_ptr, Account::None) {
                break XmlError::AmplificationLimitBreach;
            }

            match tok {
                XML_TOK_PARAM_ENTITY_REF => {
                    // In the internal subset, PE references are not legal within markup
                    // declarations, e.g entity values in this case.
                    self.event_ptr = Some(entity_text_ptr);
                    break XmlError::ParamEntityRef;
                }
                XML_TOK_NONE => break XmlError::None,
                XML_TOK_ENTITY_REF | XML_TOK_DATA_CHARS => {
                    pool_append(enc, b, entity_text_ptr, next, pool);
                }
                XML_TOK_TRAILING_CR | XML_TOK_DATA_NEWLINE => {
                    if tok == XML_TOK_TRAILING_CR {
                        next = entity_text_ptr + mbpc;
                    }
                    pool.push(0xA);
                }
                XML_TOK_CHAR_REF => {
                    let n = enc.char_ref_number(b, entity_text_ptr);
                    if n < 0 {
                        self.event_ptr = Some(entity_text_ptr);
                        break XmlError::BadCharRef;
                    }
                    let mut buf = [0u8; XML_UTF8_ENCODE_MAX];
                    let n = xml_utf8_encode(n, &mut buf);
                    pool.extend_from_slice(&buf[..n]);
                }
                XML_TOK_PARTIAL => {
                    self.event_ptr = Some(entity_text_ptr);
                    break XmlError::InvalidToken;
                }
                XML_TOK_INVALID => {
                    self.event_ptr = Some(next);
                    break XmlError::InvalidToken;
                }
                _ => {
                    self.event_ptr = Some(entity_text_ptr);
                    break XmlError::UnexpectedState;
                }
            }
            entity_text_ptr = next;
        };
        self.prolog_state.in_entity_value = old_in_entity_value;
        result
    }

    // ---- content ----

    /// Port of `contentProcessor` (xmlparse.c:3160-3172).
    fn content_processor(
        &mut self,
        b: &[u8],
        start: usize,
        end: usize,
        end_ptr: &mut usize,
    ) -> HResult<E> {
        let enc = self.known_encoding();
        let have_more = !self.final_buffer;
        self.do_content(
            Src::Main,
            0,
            enc,
            b,
            start,
            end,
            end_ptr,
            have_more,
            Account::Direct,
        )
    }

    /// Port of `doContent` (xmlparse.c:3295-3778) without namespace processing.
    #[allow(clippy::too_many_arguments)]
    fn do_content(
        &mut self,
        src: Src,
        start_tag_level: i32,
        enc: &'static Encoding,
        b: &[u8],
        mut s: usize,
        end: usize,
        next_ptr: &mut usize,
        have_more: bool,
        account: Account,
    ) -> HResult<E> {
        let mbpc = enc.min_bytes_per_char;
        self.set_event_ptr(src, s);

        loop {
            // XmlContentTok doesn't always set the last arg
            let mut next = s;
            let tok = enc.content_tok(b, s, end, &mut next);
            let account_after = if tok == XML_TOK_TRAILING_RSQB || tok == XML_TOK_TRAILING_CR {
                if have_more { s } else { end }
            } else {
                next
            };
            if !self.accounting_diff_tolerated(tok, account_after - s, account) {
                return Ok(XmlError::AmplificationLimitBreach);
            }
            self.set_event_end_ptr(src, next);
            match tok {
                XML_TOK_TRAILING_CR => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    self.set_event_end_ptr(src, end);
                    self.report_char_data(b"\n")?;
                    // We are at the end of the final buffer, should we check for
                    // XML_SUSPENDED, XML_FINISHED?
                    if start_tag_level == 0 {
                        return Ok(XmlError::NoElements);
                    }
                    if self.tag_level != start_tag_level {
                        return Ok(XmlError::AsyncEntity);
                    }
                    *next_ptr = end;
                    return Ok(XmlError::None);
                }
                XML_TOK_NONE => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    if start_tag_level > 0 {
                        if self.tag_level != start_tag_level {
                            return Ok(XmlError::AsyncEntity);
                        }
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::NoElements);
                }
                XML_TOK_INVALID => {
                    self.set_event_ptr(src, next);
                    return Ok(XmlError::InvalidToken);
                }
                XML_TOK_PARTIAL => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::UnclosedToken);
                }
                XML_TOK_PARTIAL_CHAR => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::PartialChar);
                }
                XML_TOK_ENTITY_REF => {
                    let ch = enc.predefined_entity_name(b, s + mbpc, next - mbpc);
                    if ch != 0 {
                        // NOTE: We are replacing 4-6 characters original input for 1
                        // character so there is no amplification and hence recording without
                        // protection.
                        self.accounting_diff_tolerated(tok, 1, Account::EntityExpansion);
                        self.report_char_data(&[ch as u8])?;
                    } else {
                        let name = pool_store_string(enc, b, s + mbpc, next - mbpc);
                        let entity = self.dtd.general_entities.get(&name).copied();
                        // First, determine if a check for an existing declaration is needed;
                        // if yes, check that the entity exists, and that it is internal,
                        // otherwise call the skipped entity or default handler.
                        if !self.dtd.has_param_entity_refs || self.dtd.standalone {
                            match entity {
                                None => return Ok(XmlError::UndefinedEntity),
                                Some(id) if !self.dtd.entities[id].is_internal => {
                                    return Ok(XmlError::EntityDeclaredInPe);
                                }
                                Some(_) => {}
                            }
                        }
                        if let Some(id) = entity {
                            let e = &self.dtd.entities[id];
                            if e.open {
                                return Ok(XmlError::RecursiveEntityRef);
                            }
                            if e.has_notation {
                                return Ok(XmlError::BinaryEntityRef);
                            }
                            if e.text.is_some() {
                                // m_defaultExpandInternalEntities is true.
                                try_xml!(Ok::<XmlError, E>(
                                    self.process_entity(id, EntityType::Internal)
                                ));
                            }
                            // An external entity without an external entity reference handler
                            // is skipped.
                        }
                    }
                }
                XML_TOK_START_TAG_NO_ATTS | XML_TOK_START_TAG_WITH_ATTS => {
                    let raw_name = s + mbpc;
                    let raw_name_length = enc.name_length(b, raw_name);
                    let raw_name_end = raw_name + raw_name_length;
                    // A TAG and its buffer, beyond those on the free list.
                    match self.charge_growth(
                        self.tags_high_water,
                        self.tag_stack.len() + 1,
                        size_of::<Tag>() + INIT_TAG_BUF_SIZE,
                    ) {
                        Ok(high_water) => self.tags_high_water = high_water,
                        Err(result) => return Ok(result),
                    }
                    self.tag_stack.push(Tag {
                        raw_name: b[raw_name..raw_name_end].to_vec(),
                        name: pool_store_string(enc, b, raw_name, raw_name_end),
                    });
                    self.tag_level += 1;
                    let name = self.tag_stack.last().expect("pushed").name.clone();
                    let app_atts =
                        match self.store_atts(src == Src::Main, enc, b, s, &name, account) {
                            Ok(atts) => atts,
                            Err(result) => return Ok(result),
                        };
                    if let Some(handler) = self.start_element_handler.as_mut() {
                        let atts: Vec<&[u8]> = app_atts.iter().map(|a| &a[..]).collect();
                        handler(&name, &atts)?;
                    }
                }
                XML_TOK_EMPTY_ELEMENT_NO_ATTS | XML_TOK_EMPTY_ELEMENT_WITH_ATTS => {
                    let raw_name = s + mbpc;
                    let name = pool_store_string(
                        enc,
                        b,
                        raw_name,
                        raw_name + enc.name_length(b, raw_name),
                    );
                    // token spans whole start tag
                    let app_atts =
                        match self.store_atts(src == Src::Main, enc, b, s, &name, Account::None) {
                            Ok(atts) => atts,
                            Err(result) => return Ok(result),
                        };
                    if let Some(handler) = self.start_element_handler.as_mut() {
                        let atts: Vec<&[u8]> = app_atts.iter().map(|a| &a[..]).collect();
                        handler(&name, &atts)?;
                    }
                    if self.end_element_handler.is_some() {
                        if self.start_element_handler.is_some()
                            && let Some(p) = self.event_end_ptr_of(src)
                        {
                            self.set_event_ptr(src, p);
                        }
                        if let Some(handler) = self.end_element_handler.as_mut() {
                            handler(&name)?;
                        }
                    }
                    if self.tag_level == 0 {
                        if self.reenter {
                            self.processor = Processor::Epilog;
                        } else {
                            return self.epilog_processor(b, next, end, next_ptr);
                        }
                    }
                }
                XML_TOK_END_TAG => {
                    if self.tag_level == start_tag_level {
                        return Ok(XmlError::AsyncEntity);
                    }
                    let raw_name = s + mbpc * 2;
                    let len = enc.name_length(b, raw_name);
                    let tag = self.tag_stack.last().expect("tag level above zero");
                    if len != tag.raw_name.len() || b[raw_name..raw_name + len] != tag.raw_name[..]
                    {
                        self.set_event_ptr(src, raw_name);
                        return Ok(XmlError::TagMismatch);
                    }
                    let tag = self.tag_stack.pop().expect("tag level above zero");
                    self.tag_level -= 1;
                    if let Some(handler) = self.end_element_handler.as_mut() {
                        handler(&tag.name)?;
                    }
                    if self.tag_level == 0 {
                        if self.reenter {
                            self.processor = Processor::Epilog;
                        } else {
                            return self.epilog_processor(b, next, end, next_ptr);
                        }
                    }
                }
                XML_TOK_CHAR_REF => {
                    let n = enc.char_ref_number(b, s);
                    if n < 0 {
                        return Ok(XmlError::BadCharRef);
                    }
                    let mut buf = [0u8; XML_UTF8_ENCODE_MAX];
                    let len = xml_utf8_encode(n, &mut buf);
                    self.report_char_data(&buf[..len])?;
                }
                XML_TOK_XML_DECL => return Ok(XmlError::MisplacedXmlPi),
                XML_TOK_DATA_NEWLINE => self.report_char_data(b"\n")?,
                XML_TOK_CDATA_SECT_OPEN => {
                    let mut start = Some(next);
                    try_xml!(self.do_cdata_section(
                        src, enc, b, &mut start, end, next_ptr, have_more, account
                    ));
                    match start {
                        Some(after) => next = after,
                        None => {
                            self.processor = Processor::CdataSection;
                            return Ok(XmlError::None);
                        }
                    }
                }
                XML_TOK_TRAILING_RSQB => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    if self.character_data_handler.is_some() {
                        if must_convert(enc) {
                            let mut data_buf = [0u8; INIT_DATA_BUF_SIZE];
                            let mut data_ptr = 0usize;
                            let mut from = s;
                            enc.utf8_convert(
                                b,
                                &mut from,
                                end,
                                &mut data_buf,
                                &mut data_ptr,
                                INIT_DATA_BUF_SIZE,
                            );
                            self.report_char_data(&data_buf[..data_ptr])?;
                        } else {
                            self.report_char_data(&b[s..end])?;
                        }
                    }
                    // We are at the end of the final buffer, should we check for
                    // XML_SUSPENDED, XML_FINISHED?
                    if start_tag_level == 0 {
                        self.set_event_ptr(src, end);
                        return Ok(XmlError::NoElements);
                    }
                    if self.tag_level != start_tag_level {
                        self.set_event_ptr(src, end);
                        return Ok(XmlError::AsyncEntity);
                    }
                    *next_ptr = end;
                    return Ok(XmlError::None);
                }
                XML_TOK_DATA_CHARS if self.character_data_handler.is_some() => {
                    if must_convert(enc) {
                        let mut from = s;
                        loop {
                            let mut data_buf = [0u8; INIT_DATA_BUF_SIZE];
                            let mut data_ptr = 0usize;
                            let convert_res = enc.utf8_convert(
                                b,
                                &mut from,
                                next,
                                &mut data_buf,
                                &mut data_ptr,
                                INIT_DATA_BUF_SIZE,
                            );
                            self.set_event_end_ptr(src, from);
                            self.report_char_data(&data_buf[..data_ptr])?;
                            if convert_res == ConvertResult::Completed
                                || convert_res == ConvertResult::InputIncomplete
                            {
                                break;
                            }
                            self.set_event_ptr(src, from);
                        }
                    } else {
                        self.report_char_data(&b[s..next])?;
                    }
                }
                // Processing instructions and comments: reported only to their handlers.
                _ => {}
            }
            // XML_PARSING: the parser is never suspended or finished.
            if self.reenter {
                *next_ptr = next;
                return Ok(XmlError::None);
            }
            s = next;
            self.set_event_ptr(src, s);
        }
    }

    /// The attributes of the start tag at `b[att_str..]` for the application, names and
    /// values alternately, the specified ones first, then the defaulted ones.
    ///
    /// Port of `storeAtts` (xmlparse.c:3809-4275) without namespace processing. `main` is
    /// `enc == parser->m_encoding`.
    fn store_atts(
        &mut self,
        main: bool,
        enc: &'static Encoding,
        b: &[u8],
        att_str: usize,
        tag_name: &[u8],
        account: Account,
    ) -> Result<Vec<Rc<[u8]>>, XmlError> {
        // lookup the element type name
        let element_type = self.lookup_element_type(tag_name)?;
        let n_default_atts = self.dtd.elements[element_type].default_atts.len();

        // get the attributes from the tokenizer
        let n = enc.get_atts(b, att_str, &mut self.atts);

        if n + n_default_atts > self.atts.len() {
            let old_atts_size = self.atts.len();
            let new_size = n + n_default_atts + INIT_ATTS_SIZE;
            self.charge_realloc(|p| &mut p.atts_block, new_size * size_of::<Attribute>())?;
            self.atts
                .resize(n + n_default_atts + INIT_ATTS_SIZE, Attribute::default());
            if n > old_atts_size {
                enc.get_atts(b, att_str, &mut self.atts);
            }
        }

        let mut app_atts: Vec<Rc<[u8]>> = Vec::with_capacity(2 * (n + n_default_atts));
        let mut app_ids: Vec<usize> = Vec::with_capacity(n + n_default_atts);
        let mut temp_pool_used = 0usize;
        for i in 0..n {
            let curr_att = self.atts[i];
            // add the name and value to the attribute list
            let att_id = self.get_attribute_id(
                enc,
                b,
                curr_att.name,
                curr_att.name + enc.name_length(b, curr_att.name),
            )?;
            // Detect duplicate attributes by their QNames.
            if self.dtd.att_ids[att_id].specified {
                if main {
                    self.event_ptr = Some(curr_att.name);
                }
                return Err(XmlError::DuplicateAttribute);
            }
            self.dtd.att_ids[att_id].specified = true;
            app_atts.push(self.dtd.att_ids[att_id].name.clone());
            app_ids.push(att_id);
            let value = if !curr_att.normalized {
                let mut is_cdata = true;

                // figure out whether declared as other than CDATA
                if self.dtd.att_ids[att_id].maybe_tokenized
                    && let Some(da) = self.dtd.elements[element_type]
                        .default_atts
                        .iter()
                        .find(|da| da.id == att_id)
                {
                    is_cdata = da.is_cdata;
                }

                // normalize the attribute value
                self.store_attribute_value(
                    main,
                    enc,
                    is_cdata,
                    b,
                    curr_att.value_ptr,
                    curr_att.value_end,
                    false,
                    account,
                )?
            } else {
                // the value did not need normalizing
                pool_store_string(enc, b, curr_att.value_ptr, curr_att.value_end)
            };
            // The temporary pool's bytes, kept for the next tags.
            temp_pool_used += value.len() + 1;
            self.temp_pool_high_water =
                self.charge_growth(self.temp_pool_high_water, temp_pool_used, 1)?;
            app_atts.push(value.into());
        }

        // set-up for XML_GetSpecifiedAttributeCount and XML_GetIdAttributeIndex
        self.n_specified_atts = app_atts.len() as i32;
        match self.dtd.elements[element_type].id_att {
            Some(id_att) if self.dtd.att_ids[id_att].specified => {
                if let Some(i) = app_ids.iter().position(|&id| id == id_att) {
                    self.id_att_index = 2 * i as i32;
                }
            }
            _ => self.id_att_index = -1,
        }

        // do attribute defaulting
        for i in 0..n_default_atts {
            let da = &self.dtd.elements[element_type].default_atts[i];
            let id = da.id;
            if let Some(value) = &da.value
                && !self.dtd.att_ids[id].specified
            {
                let value = value.clone();
                self.dtd.att_ids[id].specified = true;
                app_atts.push(self.dtd.att_ids[id].name.clone());
                app_ids.push(id);
                app_atts.push(value);
            }
        }

        // clear flags that say whether attributes were specified
        for id in app_ids {
            self.dtd.att_ids[id].specified = false;
        }
        Ok(app_atts)
    }

    /// The value of an attribute, `b[ptr..end]` in `enc`, normalized: references replaced,
    /// white space made spaces, and for a value not declared CDATA, runs of spaces made one
    /// and spaces at the ends removed.
    ///
    /// Port of `storeAttributeValue` (xmlparse.c:6476-6551). `from_prolog` is
    /// `pool == &dtd->pool`: a default value in an attribute list declaration.
    #[allow(clippy::too_many_arguments)]
    fn store_attribute_value(
        &mut self,
        main: bool,
        enc: &'static Encoding,
        is_cdata: bool,
        b: &[u8],
        ptr: usize,
        end: usize,
        from_prolog: bool,
        account: Account,
    ) -> Result<Vec<u8>, XmlError> {
        let mut pool = Vec::new();
        let mut next = ptr;
        let mut result;

        loop {
            match self.open_attribute_entities.last() {
                None => {
                    let start = next;
                    result = self.append_attribute_value(
                        main,
                        enc,
                        is_cdata,
                        b,
                        start,
                        end,
                        &mut pool,
                        from_prolog,
                        account,
                        &mut next,
                    );
                }
                Some(open_entity) => {
                    let id = open_entity.entity;
                    let entity = &self.dtd.entities[id];
                    let text = entity.text.clone().expect("an internal entity");
                    let text_start = entity.processed;
                    let text_end = text.len();
                    // Set a safe default value in case 'next' does not get set
                    let mut next_in_entity = text_start;
                    if entity.has_more {
                        result = self.append_attribute_value(
                            false,
                            self.internal_encoding,
                            is_cdata,
                            &text,
                            text_start,
                            text_end,
                            &mut pool,
                            from_prolog,
                            Account::EntityExpansion,
                            &mut next_in_entity,
                        );
                        if result != XmlError::None {
                            break;
                        }
                        // Check if entity is complete, if not, mark down how much of it is
                        // processed.
                        if text_end != next_in_entity {
                            self.dtd.entities[id].processed = next_in_entity;
                            continue;
                        }

                        // Entity is complete. We cannot close it here since we need to first
                        // process its possible inner entities.
                        self.dtd.entities[id].has_more = false;
                        continue;
                    }

                    // Remove fully processed openEntity from open entity list.
                    self.dtd.entities[id].open = false;
                    self.open_attribute_entities.pop();
                    result = XmlError::None;
                }
            }

            // Break if an error occurred or there is nothing left to process
            if result != XmlError::None || (self.open_attribute_entities.is_empty() && end == next)
            {
                break;
            }
        }

        if result != XmlError::None {
            return Err(result);
        }
        if !is_cdata && pool.last() == Some(&0x20) {
            pool.pop();
        }
        Ok(pool)
    }

    /// Port of `appendAttributeValue` (xmlparse.c:6553-6745).
    #[allow(clippy::too_many_arguments)]
    fn append_attribute_value(
        &mut self,
        main: bool,
        enc: &'static Encoding,
        is_cdata: bool,
        b: &[u8],
        mut ptr: usize,
        end: usize,
        pool: &mut Vec<u8>,
        from_prolog: bool,
        account: Account,
        next_ptr: &mut usize,
    ) -> XmlError {
        let mbpc = enc.min_bytes_per_char;
        loop {
            // XmlAttributeValueTok doesn't always set the last arg
            let mut next = ptr;
            let tok = enc.attribute_value_tok(b, ptr, end, &mut next);
            if !self.accounting_diff_tolerated(tok, next - ptr, account) {
                return XmlError::AmplificationLimitBreach;
            }
            match tok {
                XML_TOK_NONE => {
                    *next_ptr = next;
                    return XmlError::None;
                }
                XML_TOK_INVALID => {
                    if main {
                        self.event_ptr = Some(next);
                    }
                    return XmlError::InvalidToken;
                }
                XML_TOK_PARTIAL => {
                    if main {
                        self.event_ptr = Some(ptr);
                    }
                    return XmlError::InvalidToken;
                }
                XML_TOK_CHAR_REF => {
                    let n = enc.char_ref_number(b, ptr);
                    if n < 0 {
                        if main {
                            self.event_ptr = Some(ptr);
                        }
                        return XmlError::BadCharRef;
                    }
                    if !(!is_cdata && n == 0x20 && (pool.is_empty() || pool.last() == Some(&0x20)))
                    {
                        let mut buf = [0u8; XML_UTF8_ENCODE_MAX];
                        let n = xml_utf8_encode(n, &mut buf);
                        pool.extend_from_slice(&buf[..n]);
                    }
                }
                XML_TOK_DATA_CHARS => pool_append(enc, b, ptr, next, pool),
                XML_TOK_TRAILING_CR | XML_TOK_ATTRIBUTE_VALUE_S | XML_TOK_DATA_NEWLINE => {
                    if tok == XML_TOK_TRAILING_CR {
                        next = ptr + mbpc;
                    }
                    if !(!is_cdata && (pool.is_empty() || pool.last() == Some(&0x20))) {
                        pool.push(0x20);
                    }
                }
                XML_TOK_ENTITY_REF => {
                    let ch = enc.predefined_entity_name(b, ptr + mbpc, next - mbpc);
                    if ch != 0 {
                        // NOTE: We are replacing 4-6 characters original input for 1
                        // character so there is no amplification and hence recording without
                        // protection.
                        self.accounting_diff_tolerated(tok, 1, Account::EntityExpansion);
                        pool.push(ch as u8);
                    } else {
                        let name = pool_store_string(enc, b, ptr + mbpc, next - mbpc);
                        let entity = self.dtd.general_entities.get(&name).copied();
                        // First, determine if a check for an existing declaration is needed;
                        // if yes, check that the entity exists, and that it is internal.
                        let check_entity_decl = if from_prolog {
                            self.prolog_state.document_entity
                                && (if self.dtd.standalone {
                                    self.open_internal_entities.is_empty()
                                } else {
                                    !self.dtd.has_param_entity_refs
                                })
                        } else {
                            !self.dtd.has_param_entity_refs || self.dtd.standalone
                        };
                        let id = match entity {
                            None if check_entity_decl => return XmlError::UndefinedEntity,
                            Some(id) if check_entity_decl && !self.dtd.entities[id].is_internal => {
                                return XmlError::EntityDeclaredInPe;
                            }
                            Some(id) => Some(id),
                            None => None,
                        };
                        // An undeclared entity that needs no check is skipped.
                        if let Some(id) = id {
                            let e = &self.dtd.entities[id];
                            if e.open {
                                if main {
                                    self.event_ptr = Some(ptr);
                                }
                                return XmlError::RecursiveEntityRef;
                            }
                            if e.has_notation {
                                if main {
                                    self.event_ptr = Some(ptr);
                                }
                                return XmlError::BinaryEntityRef;
                            }
                            if e.text.is_none() {
                                if main {
                                    self.event_ptr = Some(ptr);
                                }
                                return XmlError::AttributeExternalEntityRef;
                            }
                            let result = self.process_entity(id, EntityType::Attribute);
                            if result == XmlError::None {
                                *next_ptr = next;
                            }
                            return result;
                        }
                    }
                }
                _ => {
                    // The only token returned by XmlAttributeValueTok() that does not have an
                    // explicit case here is XML_TOK_PARTIAL_CHAR, which previous tokenisers
                    // will have already recognised and rejected.
                    if main {
                        self.event_ptr = Some(ptr);
                    }
                    return XmlError::UnexpectedState;
                }
            }
            ptr = next;
        }
    }

    // ---- CDATA sections, the epilog, entities ----

    /// Port of `cdataSectionProcessor` (xmlparse.c:4557-4575).
    fn cdata_section_processor(
        &mut self,
        b: &[u8],
        start: usize,
        end: usize,
        end_ptr: &mut usize,
    ) -> HResult<E> {
        let enc = self.known_encoding();
        let have_more = !self.final_buffer;
        let mut start = Some(start);
        try_xml!(self.do_cdata_section(
            Src::Main,
            enc,
            b,
            &mut start,
            end,
            end_ptr,
            have_more,
            Account::Direct
        ));
        if let Some(start) = start {
            self.processor = Processor::Content;
            return self.content_processor(b, start, end, end_ptr);
        }
        Ok(XmlError::None)
    }

    /// Port of `doCdataSection` (xmlparse.c:4580-4704): `*start_ptr` becomes `Some` if the
    /// section is closed, and `None` if it is not yet.
    #[allow(clippy::too_many_arguments)]
    fn do_cdata_section(
        &mut self,
        src: Src,
        enc: &'static Encoding,
        b: &[u8],
        start_ptr: &mut Option<usize>,
        end: usize,
        next_ptr: &mut usize,
        have_more: bool,
        account: Account,
    ) -> HResult<E> {
        let mut s = start_ptr.expect("a start");
        self.set_event_ptr(src, s);
        *start_ptr = None;

        loop {
            // in case of XML_TOK_NONE or XML_TOK_PARTIAL
            let mut next = s;
            let tok = enc.cdata_section_tok(b, s, end, &mut next);
            if !self.accounting_diff_tolerated(tok, next - s, account) {
                return Ok(XmlError::AmplificationLimitBreach);
            }
            self.set_event_end_ptr(src, next);
            match tok {
                XML_TOK_CDATA_SECT_CLOSE => {
                    *start_ptr = Some(next);
                    *next_ptr = next;
                    return Ok(XmlError::None);
                }
                XML_TOK_DATA_NEWLINE => self.report_char_data(b"\n")?,
                XML_TOK_DATA_CHARS => {
                    if self.character_data_handler.is_some() {
                        if must_convert(enc) {
                            loop {
                                let mut data_buf = [0u8; INIT_DATA_BUF_SIZE];
                                let mut data_ptr = 0usize;
                                let convert_res = enc.utf8_convert(
                                    b,
                                    &mut s,
                                    next,
                                    &mut data_buf,
                                    &mut data_ptr,
                                    INIT_DATA_BUF_SIZE,
                                );
                                self.set_event_end_ptr(src, next);
                                self.report_char_data(&data_buf[..data_ptr])?;
                                if convert_res == ConvertResult::Completed
                                    || convert_res == ConvertResult::InputIncomplete
                                {
                                    break;
                                }
                                self.set_event_ptr(src, s);
                            }
                        } else {
                            self.report_char_data(&b[s..next])?;
                        }
                    }
                }
                XML_TOK_INVALID => {
                    self.set_event_ptr(src, next);
                    return Ok(XmlError::InvalidToken);
                }
                XML_TOK_PARTIAL_CHAR => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::PartialChar);
                }
                XML_TOK_PARTIAL | XML_TOK_NONE => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::UnclosedCdataSection);
                }
                _ => {
                    // Every token returned by XmlCdataSectionTok() has its own explicit case.
                    self.set_event_ptr(src, next);
                    return Ok(XmlError::UnexpectedState);
                }
            }

            // XML_PARSING: the parser is never suspended or finished.
            if self.reenter {
                return Ok(XmlError::UnexpectedState);
            }
            s = next;
            self.set_event_ptr(src, s);
        }
    }

    /// Port of `epilogProcessor` (xmlparse.c:6250-6326) without handlers.
    fn epilog_processor(
        &mut self,
        b: &[u8],
        mut s: usize,
        end: usize,
        next_ptr: &mut usize,
    ) -> HResult<E> {
        self.processor = Processor::Epilog;
        self.event_ptr = Some(s);
        let enc = self.known_encoding();
        loop {
            let mut next = s;
            let tok = enc.prolog_tok(b, s, end, &mut next);
            if !self.accounting_diff_tolerated(tok, next - s, Account::Direct) {
                return Ok(XmlError::AmplificationLimitBreach);
            }
            self.event_end_ptr = Some(next);
            match tok {
                // report partial linebreak - it might be the last token
                t if t == -XML_TOK_PROLOG_S => {
                    *next_ptr = next;
                    return Ok(XmlError::None);
                }
                XML_TOK_NONE => {
                    *next_ptr = s;
                    return Ok(XmlError::None);
                }
                XML_TOK_PROLOG_S | XML_TOK_PI | XML_TOK_COMMENT => {}
                XML_TOK_INVALID => {
                    self.event_ptr = Some(next);
                    return Ok(XmlError::InvalidToken);
                }
                XML_TOK_PARTIAL => {
                    if !self.final_buffer {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::UnclosedToken);
                }
                XML_TOK_PARTIAL_CHAR => {
                    if !self.final_buffer {
                        *next_ptr = s;
                        return Ok(XmlError::None);
                    }
                    return Ok(XmlError::PartialChar);
                }
                _ => return Ok(XmlError::JunkAfterDocElement),
            }
            // XML_PARSING: the parser is never suspended or finished.
            if self.reenter {
                return Ok(XmlError::UnexpectedState);
            }
            s = next;
            self.event_ptr = Some(s);
        }
    }

    /// Opens the entity `id` for expansion in content (`Internal`) or in an attribute value.
    ///
    /// Port of `processEntity` (xmlparse.c:6328-6387) for general entities.
    fn process_entity(&mut self, id: usize, ty: EntityType) -> XmlError {
        // An OPEN_INTERNAL_ENTITY, beyond those on the free lists.
        let open = self.open_internal_entities.len() + self.open_attribute_entities.len();
        match self.charge_growth(
            self.open_entities_high_water,
            open + 1,
            size_of::<OpenEntity>(),
        ) {
            Ok(high_water) => self.open_entities_high_water = high_water,
            Err(result) => return result,
        }
        let open_entity = OpenEntity {
            internal_event_ptr: None,
            internal_event_end_ptr: None,
            entity: id,
            start_tag_level: self.tag_level,
        };
        let entity = &mut self.dtd.entities[id];
        entity.open = true;
        entity.has_more = true;
        entity.processed = 0;
        match ty {
            EntityType::Internal => {
                self.processor = Processor::InternalEntity;
                self.open_internal_entities.push(open_entity);
                // Only internal entities make use of the reenter flag
                self.reenter = true;
            }
            EntityType::Attribute => self.open_attribute_entities.push(open_entity),
        }
        XmlError::None
    }

    /// Port of `internalEntityProcessor` (xmlparse.c:6389-6465) for general entities.
    fn internal_entity_processor(&mut self) -> HResult<E> {
        let Some(open_entity) = self.open_internal_entities.last() else {
            return Ok(XmlError::UnexpectedState);
        };
        let index = self.open_internal_entities.len() - 1;
        let id = open_entity.entity;
        let start_tag_level = open_entity.start_tag_level;

        // This will return early
        let entity = &self.dtd.entities[id];
        if entity.has_more {
            let text = entity.text.clone().expect("an internal entity");
            let text_start = entity.processed;
            let text_end = text.len();
            // Set a safe default value in case 'next' does not get set
            let mut next = text_start;

            let result = self.do_content(
                Src::Entity(index),
                start_tag_level,
                self.internal_encoding,
                &text,
                text_start,
                text_end,
                &mut next,
                false,
                Account::EntityExpansion,
            )?;

            if result != XmlError::None {
                return Ok(result);
            }
            // Check if entity is complete, if not, mark down how much of it is processed
            if text_end != next && self.reenter {
                self.dtd.entities[id].processed = next;
                return Ok(result);
            }

            // Entity is complete. We cannot close it here since we need to first process its
            // possible inner entities.
            self.dtd.entities[id].has_more = false;
            self.reenter = true;
            return Ok(result);
        } // End of entity processing, "if" block will return here

        // Remove fully processed openEntity from open entity list.
        self.dtd.entities[id].open = false;
        self.open_internal_entities.pop();

        if self.open_internal_entities.is_empty() {
            self.processor = Processor::Content;
        }
        self.reenter = true;
        Ok(XmlError::None)
    }
}

impl<E> std::fmt::Debug for Parser<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parser")
            .field("processor", &self.processor)
            .field("error_code", &self.error_code)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl<E> Default for Parser<'_, E> {
    fn default() -> Self {
        Parser::new()
    }
}

#[cfg(test)]
#[path = "xmlparse_tests.rs"]
mod tests;
