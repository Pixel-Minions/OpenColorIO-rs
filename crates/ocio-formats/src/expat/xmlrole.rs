// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The prolog's state machine: a port of `lib/xmlrole.c` and `xmlrole.h` (expat 2.7.2), built
//! with `XML_DTD`. Given the prolog's tokens one by one, it says what role each plays
//! (`XML_ROLE_*`): the XML declaration, the document type declaration and its internal
//! subset (entity, attribute list, element and notation declarations), or an error.
//!
//! Upstream's state is a pointer to the handler function for the next token; here it is
//! [`Handler`], and [`PrologState::token_role`] is `XmlTokenRole`. The keywords
//! (`KW_DOCTYPE` and the others, xmlrole.c:61-110) are byte strings.

use super::xmltok::Encoding;
use super::xmltok_impl::{
    XML_TOK_BOM, XML_TOK_CLOSE_BRACKET, XML_TOK_CLOSE_PAREN, XML_TOK_CLOSE_PAREN_ASTERISK,
    XML_TOK_CLOSE_PAREN_PLUS, XML_TOK_CLOSE_PAREN_QUESTION, XML_TOK_COMMA, XML_TOK_COMMENT,
    XML_TOK_COND_SECT_CLOSE, XML_TOK_COND_SECT_OPEN, XML_TOK_DECL_CLOSE, XML_TOK_DECL_OPEN,
    XML_TOK_INSTANCE_START, XML_TOK_LITERAL, XML_TOK_NAME, XML_TOK_NAME_ASTERISK,
    XML_TOK_NAME_PLUS, XML_TOK_NAME_QUESTION, XML_TOK_NMTOKEN, XML_TOK_NONE, XML_TOK_OPEN_BRACKET,
    XML_TOK_OPEN_PAREN, XML_TOK_OR, XML_TOK_PARAM_ENTITY_REF, XML_TOK_PERCENT, XML_TOK_PI,
    XML_TOK_POUND_NAME, XML_TOK_PREFIXED_NAME, XML_TOK_PROLOG_S, XML_TOK_XML_DECL,
};

// The roles (xmlrole.h:46-110).
pub const XML_ROLE_ERROR: i32 = -1;
pub const XML_ROLE_NONE: i32 = 0;
pub const XML_ROLE_XML_DECL: i32 = 1;
pub const XML_ROLE_INSTANCE_START: i32 = 2;
pub const XML_ROLE_DOCTYPE_NONE: i32 = 3;
pub const XML_ROLE_DOCTYPE_NAME: i32 = 4;
pub const XML_ROLE_DOCTYPE_SYSTEM_ID: i32 = 5;
pub const XML_ROLE_DOCTYPE_PUBLIC_ID: i32 = 6;
pub const XML_ROLE_DOCTYPE_INTERNAL_SUBSET: i32 = 7;
pub const XML_ROLE_DOCTYPE_CLOSE: i32 = 8;
pub const XML_ROLE_GENERAL_ENTITY_NAME: i32 = 9;
pub const XML_ROLE_PARAM_ENTITY_NAME: i32 = 10;
pub const XML_ROLE_ENTITY_NONE: i32 = 11;
pub const XML_ROLE_ENTITY_VALUE: i32 = 12;
pub const XML_ROLE_ENTITY_SYSTEM_ID: i32 = 13;
pub const XML_ROLE_ENTITY_PUBLIC_ID: i32 = 14;
pub const XML_ROLE_ENTITY_COMPLETE: i32 = 15;
pub const XML_ROLE_ENTITY_NOTATION_NAME: i32 = 16;
pub const XML_ROLE_NOTATION_NONE: i32 = 17;
pub const XML_ROLE_NOTATION_NAME: i32 = 18;
pub const XML_ROLE_NOTATION_SYSTEM_ID: i32 = 19;
pub const XML_ROLE_NOTATION_NO_SYSTEM_ID: i32 = 20;
pub const XML_ROLE_NOTATION_PUBLIC_ID: i32 = 21;
pub const XML_ROLE_ATTRIBUTE_NAME: i32 = 22;
pub const XML_ROLE_ATTRIBUTE_TYPE_CDATA: i32 = 23;
pub const XML_ROLE_ATTRIBUTE_TYPE_ID: i32 = 24;
pub const XML_ROLE_ATTRIBUTE_TYPE_IDREF: i32 = 25;
pub const XML_ROLE_ATTRIBUTE_TYPE_IDREFS: i32 = 26;
pub const XML_ROLE_ATTRIBUTE_TYPE_ENTITY: i32 = 27;
pub const XML_ROLE_ATTRIBUTE_TYPE_ENTITIES: i32 = 28;
pub const XML_ROLE_ATTRIBUTE_TYPE_NMTOKEN: i32 = 29;
pub const XML_ROLE_ATTRIBUTE_TYPE_NMTOKENS: i32 = 30;
pub const XML_ROLE_ATTRIBUTE_ENUM_VALUE: i32 = 31;
pub const XML_ROLE_ATTRIBUTE_NOTATION_VALUE: i32 = 32;
pub const XML_ROLE_ATTLIST_NONE: i32 = 33;
pub const XML_ROLE_ATTLIST_ELEMENT_NAME: i32 = 34;
pub const XML_ROLE_IMPLIED_ATTRIBUTE_VALUE: i32 = 35;
pub const XML_ROLE_REQUIRED_ATTRIBUTE_VALUE: i32 = 36;
pub const XML_ROLE_DEFAULT_ATTRIBUTE_VALUE: i32 = 37;
pub const XML_ROLE_FIXED_ATTRIBUTE_VALUE: i32 = 38;
pub const XML_ROLE_ELEMENT_NONE: i32 = 39;
pub const XML_ROLE_ELEMENT_NAME: i32 = 40;
pub const XML_ROLE_CONTENT_ANY: i32 = 41;
pub const XML_ROLE_CONTENT_EMPTY: i32 = 42;
pub const XML_ROLE_CONTENT_PCDATA: i32 = 43;
pub const XML_ROLE_GROUP_OPEN: i32 = 44;
pub const XML_ROLE_GROUP_CLOSE: i32 = 45;
pub const XML_ROLE_GROUP_CLOSE_REP: i32 = 46;
pub const XML_ROLE_GROUP_CLOSE_OPT: i32 = 47;
pub const XML_ROLE_GROUP_CLOSE_PLUS: i32 = 48;
pub const XML_ROLE_GROUP_CHOICE: i32 = 49;
pub const XML_ROLE_GROUP_SEQUENCE: i32 = 50;
pub const XML_ROLE_CONTENT_ELEMENT: i32 = 51;
pub const XML_ROLE_CONTENT_ELEMENT_REP: i32 = 52;
pub const XML_ROLE_CONTENT_ELEMENT_OPT: i32 = 53;
pub const XML_ROLE_CONTENT_ELEMENT_PLUS: i32 = 54;
pub const XML_ROLE_PI: i32 = 55;
pub const XML_ROLE_COMMENT: i32 = 56;
pub const XML_ROLE_TEXT_DECL: i32 = 57;
pub const XML_ROLE_IGNORE_SECT: i32 = 58;
pub const XML_ROLE_INNER_PARAM_ENTITY_REF: i32 = 59;
pub const XML_ROLE_PARAM_ENTITY_REF: i32 = 60;

/// The handler for the next token: upstream's function pointer (xmlrole.c:124-134).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Prolog0,
    Prolog1,
    Prolog2,
    Doctype0,
    Doctype1,
    Doctype2,
    Doctype3,
    Doctype4,
    Doctype5,
    InternalSubset,
    ExternalSubset0,
    ExternalSubset1,
    Entity0,
    Entity1,
    Entity2,
    Entity3,
    Entity4,
    Entity5,
    Entity6,
    Entity7,
    Entity8,
    Entity9,
    Entity10,
    Notation0,
    Notation1,
    Notation2,
    Notation3,
    Notation4,
    Attlist0,
    Attlist1,
    Attlist2,
    Attlist3,
    Attlist4,
    Attlist5,
    Attlist6,
    Attlist7,
    Attlist8,
    Attlist9,
    Element0,
    Element1,
    Element2,
    Element3,
    Element4,
    Element5,
    Element6,
    Element7,
    CondSect0,
    CondSect1,
    CondSect2,
    DeclClose,
    Error,
}

/// The prolog's state.
///
/// Port of `PROLOG_STATE` (xmlrole.h:112-122), with `XML_DTD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrologState {
    /// `handler`.
    pub handler: Handler,
    /// `level`: the depth of nested groups in an element declaration.
    pub level: u32,
    /// `role_none`: the role of white space and of the end of the declaration `declClose`
    /// finishes.
    pub role_none: i32,
    /// `includeLevel`: the depth of nested INCLUDE sections.
    pub include_level: u32,
    /// `documentEntity`: parsing the document, not an external entity.
    pub document_entity: bool,
    /// `inEntityValue`: inside an entity value (set by the parser).
    pub in_entity_value: bool,
}

/// The type keywords of `attlist2`, in the order of their roles (xmlrole.c:746-749).
const ATTRIBUTE_TYPES: [&[u8]; 8] = [
    b"CDATA",
    b"ID",
    b"IDREF",
    b"IDREFS",
    b"ENTITY",
    b"ENTITIES",
    b"NMTOKEN",
    b"NMTOKENS",
];

impl PrologState {
    /// The state of a document's prolog.
    ///
    /// Port of `XmlPrologStateInit` (xmlrole.c:1236-1244). Upstream leaves `level` and
    /// `role_none` unset until a handler sets them before they are read.
    pub fn new() -> PrologState {
        PrologState {
            handler: Handler::Prolog0,
            level: 0,
            role_none: 0,
            include_level: 0,
            document_entity: true,
            in_entity_value: false,
        }
    }

    /// The state of an external entity's: the external subset.
    ///
    /// Port of `XmlPrologStateInitExternalEntity` (xmlrole.c:1246-1253).
    pub fn new_external_entity() -> PrologState {
        PrologState {
            handler: Handler::ExternalSubset0,
            document_entity: false,
            ..PrologState::new()
        }
    }

    /// `setTopLevel(state)` (xmlrole.c:116-122): back to the internal subset, or to the
    /// external subset in an external entity.
    fn set_top_level(&mut self) {
        self.handler = if self.document_entity {
            Handler::InternalSubset
        } else {
            Handler::ExternalSubset1
        };
    }

    /// The role of the token `tok`, `b[ptr..end]` in `enc`.
    ///
    /// Port of `XmlTokenRole(state, tok, ptr, end, enc)` (xmlrole.h:130-131), and of the
    /// handlers (xmlrole.c:140-1233).
    pub fn token_role(
        &mut self,
        tok: i32,
        b: &[u8],
        ptr: usize,
        end: usize,
        enc: &Encoding,
    ) -> i32 {
        let matches = |p: usize, kw: &[u8]| enc.name_matches_ascii(b, p, end, kw);
        let mbpc = enc.min_bytes_per_char;
        match self.handler {
            // prolog0 (xmlrole.c:140-169)
            Handler::Prolog0 => match tok {
                XML_TOK_PROLOG_S => {
                    self.handler = Handler::Prolog1;
                    return XML_ROLE_NONE;
                }
                XML_TOK_XML_DECL => {
                    self.handler = Handler::Prolog1;
                    return XML_ROLE_XML_DECL;
                }
                XML_TOK_PI => {
                    self.handler = Handler::Prolog1;
                    return XML_ROLE_PI;
                }
                XML_TOK_COMMENT => {
                    self.handler = Handler::Prolog1;
                    return XML_ROLE_COMMENT;
                }
                XML_TOK_BOM => return XML_ROLE_NONE,
                XML_TOK_DECL_OPEN => {
                    if matches(ptr + 2 * mbpc, b"DOCTYPE") {
                        self.handler = Handler::Doctype0;
                        return XML_ROLE_DOCTYPE_NONE;
                    }
                }
                XML_TOK_INSTANCE_START => {
                    self.handler = Handler::Error;
                    return XML_ROLE_INSTANCE_START;
                }
                _ => {}
            },
            // prolog1 (xmlrole.c:171-201)
            Handler::Prolog1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NONE,
                XML_TOK_PI => return XML_ROLE_PI,
                XML_TOK_COMMENT => return XML_ROLE_COMMENT,
                // This case can never arise (LCOV_EXCL_LINE).
                XML_TOK_BOM => return XML_ROLE_NONE,
                XML_TOK_DECL_OPEN => {
                    if matches(ptr + 2 * mbpc, b"DOCTYPE") {
                        self.handler = Handler::Doctype0;
                        return XML_ROLE_DOCTYPE_NONE;
                    }
                }
                XML_TOK_INSTANCE_START => {
                    self.handler = Handler::Error;
                    return XML_ROLE_INSTANCE_START;
                }
                _ => {}
            },
            // prolog2 (xmlrole.c:203-221)
            Handler::Prolog2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NONE,
                XML_TOK_PI => return XML_ROLE_PI,
                XML_TOK_COMMENT => return XML_ROLE_COMMENT,
                XML_TOK_INSTANCE_START => {
                    self.handler = Handler::Error;
                    return XML_ROLE_INSTANCE_START;
                }
                _ => {}
            },
            // doctype0 (xmlrole.c:223-238)
            Handler::Doctype0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Doctype1;
                    return XML_ROLE_DOCTYPE_NAME;
                }
                _ => {}
            },
            // doctype1 (xmlrole.c:240-264)
            Handler::Doctype1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_OPEN_BRACKET => {
                    self.handler = Handler::InternalSubset;
                    return XML_ROLE_DOCTYPE_INTERNAL_SUBSET;
                }
                XML_TOK_DECL_CLOSE => {
                    self.handler = Handler::Prolog2;
                    return XML_ROLE_DOCTYPE_CLOSE;
                }
                XML_TOK_NAME => {
                    if matches(ptr, b"SYSTEM") {
                        self.handler = Handler::Doctype3;
                        return XML_ROLE_DOCTYPE_NONE;
                    }
                    if matches(ptr, b"PUBLIC") {
                        self.handler = Handler::Doctype2;
                        return XML_ROLE_DOCTYPE_NONE;
                    }
                }
                _ => {}
            },
            // doctype2 (xmlrole.c:266-280)
            Handler::Doctype2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Doctype3;
                    return XML_ROLE_DOCTYPE_PUBLIC_ID;
                }
                _ => {}
            },
            // doctype3 (xmlrole.c:282-296)
            Handler::Doctype3 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Doctype4;
                    return XML_ROLE_DOCTYPE_SYSTEM_ID;
                }
                _ => {}
            },
            // doctype4 (xmlrole.c:298-315)
            Handler::Doctype4 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_OPEN_BRACKET => {
                    self.handler = Handler::InternalSubset;
                    return XML_ROLE_DOCTYPE_INTERNAL_SUBSET;
                }
                XML_TOK_DECL_CLOSE => {
                    self.handler = Handler::Prolog2;
                    return XML_ROLE_DOCTYPE_CLOSE;
                }
                _ => {}
            },
            // doctype5 (xmlrole.c:317-331)
            Handler::Doctype5 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_DOCTYPE_NONE,
                XML_TOK_DECL_CLOSE => {
                    self.handler = Handler::Prolog2;
                    return XML_ROLE_DOCTYPE_CLOSE;
                }
                _ => {}
            },
            Handler::InternalSubset => return self.internal_subset(tok, b, ptr, end, enc),
            // externalSubset0 (xmlrole.c:378-385)
            Handler::ExternalSubset0 => {
                self.handler = Handler::ExternalSubset1;
                if tok == XML_TOK_XML_DECL {
                    return XML_ROLE_TEXT_DECL;
                }
                return self.external_subset1(tok, b, ptr, end, enc);
            }
            Handler::ExternalSubset1 => return self.external_subset1(tok, b, ptr, end, enc),
            // entity0 (xmlrole.c:415-432)
            Handler::Entity0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_PERCENT => {
                    self.handler = Handler::Entity1;
                    return XML_ROLE_ENTITY_NONE;
                }
                XML_TOK_NAME => {
                    self.handler = Handler::Entity2;
                    return XML_ROLE_GENERAL_ENTITY_NAME;
                }
                _ => {}
            },
            // entity1 (xmlrole.c:434-448)
            Handler::Entity1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_NAME => {
                    self.handler = Handler::Entity7;
                    return XML_ROLE_PARAM_ENTITY_NAME;
                }
                _ => {}
            },
            // entity2 (xmlrole.c:450-472)
            Handler::Entity2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_NAME => {
                    if matches(ptr, b"SYSTEM") {
                        self.handler = Handler::Entity4;
                        return XML_ROLE_ENTITY_NONE;
                    }
                    if matches(ptr, b"PUBLIC") {
                        self.handler = Handler::Entity3;
                        return XML_ROLE_ENTITY_NONE;
                    }
                }
                XML_TOK_LITERAL => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ENTITY_NONE;
                    return XML_ROLE_ENTITY_VALUE;
                }
                _ => {}
            },
            // entity3 (xmlrole.c:474-488)
            Handler::Entity3 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Entity4;
                    return XML_ROLE_ENTITY_PUBLIC_ID;
                }
                _ => {}
            },
            // entity4 (xmlrole.c:490-504)
            Handler::Entity4 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Entity5;
                    return XML_ROLE_ENTITY_SYSTEM_ID;
                }
                _ => {}
            },
            // entity5 (xmlrole.c:506-523)
            Handler::Entity5 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_DECL_CLOSE => {
                    self.set_top_level();
                    return XML_ROLE_ENTITY_COMPLETE;
                }
                XML_TOK_NAME if matches(ptr, b"NDATA") => {
                    self.handler = Handler::Entity6;
                    return XML_ROLE_ENTITY_NONE;
                }
                _ => {}
            },
            // entity6 (xmlrole.c:525-540)
            Handler::Entity6 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_NAME => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ENTITY_NONE;
                    return XML_ROLE_ENTITY_NOTATION_NAME;
                }
                _ => {}
            },
            // entity7 (xmlrole.c:542-564)
            Handler::Entity7 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_NAME => {
                    if matches(ptr, b"SYSTEM") {
                        self.handler = Handler::Entity9;
                        return XML_ROLE_ENTITY_NONE;
                    }
                    if matches(ptr, b"PUBLIC") {
                        self.handler = Handler::Entity8;
                        return XML_ROLE_ENTITY_NONE;
                    }
                }
                XML_TOK_LITERAL => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ENTITY_NONE;
                    return XML_ROLE_ENTITY_VALUE;
                }
                _ => {}
            },
            // entity8 (xmlrole.c:566-580)
            Handler::Entity8 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Entity9;
                    return XML_ROLE_ENTITY_PUBLIC_ID;
                }
                _ => {}
            },
            // entity9 (xmlrole.c:582-596)
            Handler::Entity9 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Entity10;
                    return XML_ROLE_ENTITY_SYSTEM_ID;
                }
                _ => {}
            },
            // entity10 (xmlrole.c:598-612)
            Handler::Entity10 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ENTITY_NONE,
                XML_TOK_DECL_CLOSE => {
                    self.set_top_level();
                    return XML_ROLE_ENTITY_COMPLETE;
                }
                _ => {}
            },
            // notation0 (xmlrole.c:614-628)
            Handler::Notation0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NOTATION_NONE,
                XML_TOK_NAME => {
                    self.handler = Handler::Notation1;
                    return XML_ROLE_NOTATION_NAME;
                }
                _ => {}
            },
            // notation1 (xmlrole.c:630-648)
            Handler::Notation1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NOTATION_NONE,
                XML_TOK_NAME => {
                    if matches(ptr, b"SYSTEM") {
                        self.handler = Handler::Notation3;
                        return XML_ROLE_NOTATION_NONE;
                    }
                    if matches(ptr, b"PUBLIC") {
                        self.handler = Handler::Notation2;
                        return XML_ROLE_NOTATION_NONE;
                    }
                }
                _ => {}
            },
            // notation2 (xmlrole.c:650-664)
            Handler::Notation2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NOTATION_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Notation4;
                    return XML_ROLE_NOTATION_PUBLIC_ID;
                }
                _ => {}
            },
            // notation3 (xmlrole.c:666-681)
            Handler::Notation3 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NOTATION_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_NOTATION_NONE;
                    return XML_ROLE_NOTATION_SYSTEM_ID;
                }
                _ => {}
            },
            // notation4 (xmlrole.c:683-701)
            Handler::Notation4 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NOTATION_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_NOTATION_NONE;
                    return XML_ROLE_NOTATION_SYSTEM_ID;
                }
                XML_TOK_DECL_CLOSE => {
                    self.set_top_level();
                    return XML_ROLE_NOTATION_NO_SYSTEM_ID;
                }
                _ => {}
            },
            // attlist0 (xmlrole.c:703-718)
            Handler::Attlist0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Attlist1;
                    return XML_ROLE_ATTLIST_ELEMENT_NAME;
                }
                _ => {}
            },
            // attlist1 (xmlrole.c:720-738)
            Handler::Attlist1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_DECL_CLOSE => {
                    self.set_top_level();
                    return XML_ROLE_ATTLIST_NONE;
                }
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Attlist2;
                    return XML_ROLE_ATTRIBUTE_NAME;
                }
                _ => {}
            },
            // attlist2 (xmlrole.c:740-768)
            Handler::Attlist2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_NAME => {
                    for (i, kw) in ATTRIBUTE_TYPES.iter().enumerate() {
                        if matches(ptr, kw) {
                            self.handler = Handler::Attlist8;
                            return XML_ROLE_ATTRIBUTE_TYPE_CDATA + i as i32;
                        }
                    }
                    if matches(ptr, b"NOTATION") {
                        self.handler = Handler::Attlist5;
                        return XML_ROLE_ATTLIST_NONE;
                    }
                }
                XML_TOK_OPEN_PAREN => {
                    self.handler = Handler::Attlist3;
                    return XML_ROLE_ATTLIST_NONE;
                }
                _ => {}
            },
            // attlist3 (xmlrole.c:770-786)
            Handler::Attlist3 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_NMTOKEN | XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Attlist4;
                    return XML_ROLE_ATTRIBUTE_ENUM_VALUE;
                }
                _ => {}
            },
            // attlist4 (xmlrole.c:788-805)
            Handler::Attlist4 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_CLOSE_PAREN => {
                    self.handler = Handler::Attlist8;
                    return XML_ROLE_ATTLIST_NONE;
                }
                XML_TOK_OR => {
                    self.handler = Handler::Attlist3;
                    return XML_ROLE_ATTLIST_NONE;
                }
                _ => {}
            },
            // attlist5 (xmlrole.c:807-821)
            Handler::Attlist5 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_OPEN_PAREN => {
                    self.handler = Handler::Attlist6;
                    return XML_ROLE_ATTLIST_NONE;
                }
                _ => {}
            },
            // attlist6 (xmlrole.c:823-837)
            Handler::Attlist6 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_NAME => {
                    self.handler = Handler::Attlist7;
                    return XML_ROLE_ATTRIBUTE_NOTATION_VALUE;
                }
                _ => {}
            },
            // attlist7 (xmlrole.c:839-856)
            Handler::Attlist7 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_CLOSE_PAREN => {
                    self.handler = Handler::Attlist8;
                    return XML_ROLE_ATTLIST_NONE;
                }
                XML_TOK_OR => {
                    self.handler = Handler::Attlist6;
                    return XML_ROLE_ATTLIST_NONE;
                }
                _ => {}
            },
            // attlist8: the default value (xmlrole.c:858-887)
            Handler::Attlist8 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_POUND_NAME => {
                    if matches(ptr + mbpc, b"IMPLIED") {
                        self.handler = Handler::Attlist1;
                        return XML_ROLE_IMPLIED_ATTRIBUTE_VALUE;
                    }
                    if matches(ptr + mbpc, b"REQUIRED") {
                        self.handler = Handler::Attlist1;
                        return XML_ROLE_REQUIRED_ATTRIBUTE_VALUE;
                    }
                    if matches(ptr + mbpc, b"FIXED") {
                        self.handler = Handler::Attlist9;
                        return XML_ROLE_ATTLIST_NONE;
                    }
                }
                XML_TOK_LITERAL => {
                    self.handler = Handler::Attlist1;
                    return XML_ROLE_DEFAULT_ATTRIBUTE_VALUE;
                }
                _ => {}
            },
            // attlist9 (xmlrole.c:889-903)
            Handler::Attlist9 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ATTLIST_NONE,
                XML_TOK_LITERAL => {
                    self.handler = Handler::Attlist1;
                    return XML_ROLE_FIXED_ATTRIBUTE_VALUE;
                }
                _ => {}
            },
            // element0 (xmlrole.c:905-920)
            Handler::Element0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Element1;
                    return XML_ROLE_ELEMENT_NAME;
                }
                _ => {}
            },
            // element1 (xmlrole.c:922-946)
            Handler::Element1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_NAME => {
                    if matches(ptr, b"EMPTY") {
                        self.handler = Handler::DeclClose;
                        self.role_none = XML_ROLE_ELEMENT_NONE;
                        return XML_ROLE_CONTENT_EMPTY;
                    }
                    if matches(ptr, b"ANY") {
                        self.handler = Handler::DeclClose;
                        self.role_none = XML_ROLE_ELEMENT_NONE;
                        return XML_ROLE_CONTENT_ANY;
                    }
                }
                XML_TOK_OPEN_PAREN => {
                    self.handler = Handler::Element2;
                    self.level = 1;
                    return XML_ROLE_GROUP_OPEN;
                }
                _ => {}
            },
            // element2 (xmlrole.c:948-980)
            Handler::Element2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_POUND_NAME => {
                    if matches(ptr + mbpc, b"PCDATA") {
                        self.handler = Handler::Element3;
                        return XML_ROLE_CONTENT_PCDATA;
                    }
                }
                XML_TOK_OPEN_PAREN => {
                    self.level = 2;
                    self.handler = Handler::Element6;
                    return XML_ROLE_GROUP_OPEN;
                }
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT;
                }
                XML_TOK_NAME_QUESTION => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_OPT;
                }
                XML_TOK_NAME_ASTERISK => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_REP;
                }
                XML_TOK_NAME_PLUS => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_PLUS;
                }
                _ => {}
            },
            // element3 (xmlrole.c:982-1004)
            Handler::Element3 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_CLOSE_PAREN => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ELEMENT_NONE;
                    return XML_ROLE_GROUP_CLOSE;
                }
                XML_TOK_CLOSE_PAREN_ASTERISK => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ELEMENT_NONE;
                    return XML_ROLE_GROUP_CLOSE_REP;
                }
                XML_TOK_OR => {
                    self.handler = Handler::Element4;
                    return XML_ROLE_ELEMENT_NONE;
                }
                _ => {}
            },
            // element4 (xmlrole.c:1006-1021)
            Handler::Element4 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Element5;
                    return XML_ROLE_CONTENT_ELEMENT;
                }
                _ => {}
            },
            // element5 (xmlrole.c:1023-1041)
            Handler::Element5 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_CLOSE_PAREN_ASTERISK => {
                    self.handler = Handler::DeclClose;
                    self.role_none = XML_ROLE_ELEMENT_NONE;
                    return XML_ROLE_GROUP_CLOSE_REP;
                }
                XML_TOK_OR => {
                    self.handler = Handler::Element4;
                    return XML_ROLE_ELEMENT_NONE;
                }
                _ => {}
            },
            // element6 (xmlrole.c:1043-1070)
            Handler::Element6 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                XML_TOK_OPEN_PAREN => {
                    self.level += 1;
                    return XML_ROLE_GROUP_OPEN;
                }
                XML_TOK_NAME | XML_TOK_PREFIXED_NAME => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT;
                }
                XML_TOK_NAME_QUESTION => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_OPT;
                }
                XML_TOK_NAME_ASTERISK => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_REP;
                }
                XML_TOK_NAME_PLUS => {
                    self.handler = Handler::Element7;
                    return XML_ROLE_CONTENT_ELEMENT_PLUS;
                }
                _ => {}
            },
            // element7 (xmlrole.c:1072-1117)
            Handler::Element7 => {
                let close_role = match tok {
                    XML_TOK_PROLOG_S => return XML_ROLE_ELEMENT_NONE,
                    XML_TOK_CLOSE_PAREN => Some(XML_ROLE_GROUP_CLOSE),
                    XML_TOK_CLOSE_PAREN_ASTERISK => Some(XML_ROLE_GROUP_CLOSE_REP),
                    XML_TOK_CLOSE_PAREN_QUESTION => Some(XML_ROLE_GROUP_CLOSE_OPT),
                    XML_TOK_CLOSE_PAREN_PLUS => Some(XML_ROLE_GROUP_CLOSE_PLUS),
                    XML_TOK_COMMA => {
                        self.handler = Handler::Element6;
                        return XML_ROLE_GROUP_SEQUENCE;
                    }
                    XML_TOK_OR => {
                        self.handler = Handler::Element6;
                        return XML_ROLE_GROUP_CHOICE;
                    }
                    _ => None,
                };
                if let Some(role) = close_role {
                    self.level = self.level.wrapping_sub(1);
                    if self.level == 0 {
                        self.handler = Handler::DeclClose;
                        self.role_none = XML_ROLE_ELEMENT_NONE;
                    }
                    return role;
                }
            }
            // condSect0 (xmlrole.c:1121-1139)
            Handler::CondSect0 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NONE,
                XML_TOK_NAME => {
                    if matches(ptr, b"INCLUDE") {
                        self.handler = Handler::CondSect1;
                        return XML_ROLE_NONE;
                    }
                    if matches(ptr, b"IGNORE") {
                        self.handler = Handler::CondSect2;
                        return XML_ROLE_NONE;
                    }
                }
                _ => {}
            },
            // condSect1 (xmlrole.c:1141-1156)
            Handler::CondSect1 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NONE,
                XML_TOK_OPEN_BRACKET => {
                    self.handler = Handler::ExternalSubset1;
                    self.include_level += 1;
                    return XML_ROLE_NONE;
                }
                _ => {}
            },
            // condSect2 (xmlrole.c:1158-1172)
            Handler::CondSect2 => match tok {
                XML_TOK_PROLOG_S => return XML_ROLE_NONE,
                XML_TOK_OPEN_BRACKET => {
                    self.handler = Handler::ExternalSubset1;
                    return XML_ROLE_IGNORE_SECT;
                }
                _ => {}
            },
            // declClose (xmlrole.c:1176-1189)
            Handler::DeclClose => match tok {
                XML_TOK_PROLOG_S => return self.role_none,
                XML_TOK_DECL_CLOSE => {
                    self.set_top_level();
                    return self.role_none;
                }
                _ => {}
            },
            // error: only if the processor switch failed to happen (xmlrole.c:1212-1222,
            // LCOV_EXCL).
            Handler::Error => return XML_ROLE_NONE,
        }
        self.common(tok)
    }

    /// Port of `internalSubset` (xmlrole.c:333-375).
    fn internal_subset(
        &mut self,
        tok: i32,
        b: &[u8],
        ptr: usize,
        end: usize,
        enc: &Encoding,
    ) -> i32 {
        let mbpc = enc.min_bytes_per_char;
        match tok {
            XML_TOK_PROLOG_S => return XML_ROLE_NONE,
            XML_TOK_DECL_OPEN => {
                let name = ptr + 2 * mbpc;
                if enc.name_matches_ascii(b, name, end, b"ENTITY") {
                    self.handler = Handler::Entity0;
                    return XML_ROLE_ENTITY_NONE;
                }
                if enc.name_matches_ascii(b, name, end, b"ATTLIST") {
                    self.handler = Handler::Attlist0;
                    return XML_ROLE_ATTLIST_NONE;
                }
                if enc.name_matches_ascii(b, name, end, b"ELEMENT") {
                    self.handler = Handler::Element0;
                    return XML_ROLE_ELEMENT_NONE;
                }
                if enc.name_matches_ascii(b, name, end, b"NOTATION") {
                    self.handler = Handler::Notation0;
                    return XML_ROLE_NOTATION_NONE;
                }
            }
            XML_TOK_PI => return XML_ROLE_PI,
            XML_TOK_COMMENT => return XML_ROLE_COMMENT,
            XML_TOK_PARAM_ENTITY_REF => return XML_ROLE_PARAM_ENTITY_REF,
            XML_TOK_CLOSE_BRACKET => {
                self.handler = Handler::Doctype5;
                return XML_ROLE_DOCTYPE_NONE;
            }
            XML_TOK_NONE => return XML_ROLE_NONE,
            _ => {}
        }
        self.common(tok)
    }

    /// Port of `externalSubset1` (xmlrole.c:387-411).
    fn external_subset1(
        &mut self,
        tok: i32,
        b: &[u8],
        ptr: usize,
        end: usize,
        enc: &Encoding,
    ) -> i32 {
        match tok {
            XML_TOK_COND_SECT_OPEN => {
                self.handler = Handler::CondSect0;
                return XML_ROLE_NONE;
            }
            XML_TOK_COND_SECT_CLOSE => {
                if self.include_level != 0 {
                    self.include_level -= 1;
                    return XML_ROLE_NONE;
                }
            }
            XML_TOK_PROLOG_S => return XML_ROLE_NONE,
            XML_TOK_CLOSE_BRACKET => {}
            XML_TOK_NONE => {
                if self.include_level == 0 {
                    return XML_ROLE_NONE;
                }
            }
            _ => return self.internal_subset(tok, b, ptr, end, enc),
        }
        self.common(tok)
    }

    /// An unexpected token: an inner parameter entity reference in an external entity, an
    /// error otherwise.
    ///
    /// Port of `common` (xmlrole.c:1224-1234).
    fn common(&mut self, tok: i32) -> i32 {
        if !self.document_entity && tok == XML_TOK_PARAM_ENTITY_REF {
            return XML_ROLE_INNER_PARAM_ENTITY_REF;
        }
        self.handler = Handler::Error;
        XML_ROLE_ERROR
    }
}

impl Default for PrologState {
    fn default() -> PrologState {
        PrologState::new()
    }
}
