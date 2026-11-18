# OCIO 2.6.0: triage of the bundled library bumps

Card `ocio26-deps`, phase 2 of [the 2.6 plan](ocio-2.6-plan.md). This file lists, for each
third-party library whose version changes between the `opencolorio==2.5.2` and `2.6.0` builds,
every upstream change that reaches code the port translated or behaviour OCIO exposes, what it
does to OCIO's output, and the chunks that will port it. It is the audit trail for the card: an
item marked **no effect** says why, so a reviewer can check the reason instead of the diff.

| Library | 2.5.2 wheel | 2.6.0 (recommended) | Port today |
|---|---|---|---|
| yaml-cpp | 0.8.0 | 0.9.0 (`yaml-cpp-0.9.0`, 56e3bb550c91fd7005566f19c079cb7a503223cf) | `crates/ocio/src/yaml_cpp/` |
| expat | 2.7.2 | 2.8.4 (`R_2_8_4`, 12cf0b1f25f026a022fe728ad8f7e3d017285b80) | `crates/ocio-formats/src/expat/` |
| minizip-ng | 4.0.10 | 4.2.2 (`4.2.2`, 7b2387161c542fa9f427352dcdef76097d0d692b) | `crates/ocio/src/minizip/` |
| Imath | 3.2.1 | 3.2.3 | `crates/ocio-ops/src/imath_half.rs` |
| zlib | 1.3.1 | 1.3.2 | not translated: `flate2` 1.1.10 on `zlib-rs` 0.6.8 (D-6) |

The versions are OCIO's recommended ones (`share/cmake/modules/FindExtPackages.cmake` @
v2.6.0, OCIO PR #2350). Phase 0 confirms them in the wheels (see "Evidence after phase 0").
pystring 1.1.4, lcms2 2.17 and pybind11 don't change.

**Method.** `git diff <old> <new>` in the submodules `upstream/yaml-cpp`, `upstream/expat` and
`upstream/minizip-ng`, limited to the sources the port cites (its `Port of` comments) and those
libraries' own tests. Ignored: vendored googletest, CMake, Bazel, CI, docs and pure formatting.
Also ignored are refactors that keep behaviour: `int` to `size_t` counters that only matter
past 2^31, NULL-handle checks, `const` qualifiers, handle-to-typed-pointer edits and
`EXPAT_FALLTHROUGH` annotations. Imath 3.2.3 and zlib 1.3.2 were read from
`D:/Projects/OpenColorIO-rs-wt/_src`. For Imath 3.2.1 the port's code and citations were the
reference. zlib 1.3.1's sources aren't on this machine, so the zlib item rests on 1.3.2's
ChangeLog and on an oracle probe. OCIO's own commits that adapt to the bumps
(`2e1c9ac0`, PR #2350) were read too, because they decide which library paths OCIO reaches.

## Summary

| Library | Relevant changes | Changes OCIO-visible bytes | No effect (listed) |
|---|---:|---|---:|
| yaml-cpp 0.9.0 | 3 that OCIO sees, 4 for exactness | **Yes**: every number `Config::serialize()` writes (Y1); parse errors and accepted inputs (Y2, Y3) | 8 |
| expat 2.8.4 | 4 | Yes, for unusual inputs: two new parse errors (X1, X2) and the memory limit's threshold (X3). The OCIO 2.6 handler change needs `XML_StopParser` (X4) | 9 |
| minizip-ng 4.2.2 | 3 | Extraction only: new refusals for symlinks that escape (M1), Linux file modes (M2). Written archives unchanged | 11 |
| Imath 3.2.3 | 0 | No | 1 |
| zlib 1.3.2 | 0 expected | Not expected; one probe settles it (Z1) | 1 |

The yaml-cpp emitter change (Y1) is the one large item. From 0.9.0 on, every `float` and
`double` the emitter writes goes through a new `FpToString`: dragonbox's shortest digits,
truncated to the precision and rounded half up on the decimal digits. OCIO writes doubles at
precision 15 and floats at 7. Values whose shortest form has more digits than that are
written differently: `0.1 + 0.2` gives `0.300000000000000` where 2.5.2 gave `0.3`. Configs
serialized from computed transforms, their cache IDs and the configs inside `.ocioz` archives
all change with it.

## yaml-cpp 0.8.0 -> 0.9.0

### Changes that reach OCIO

**Y1. Numbers are written by `FpToString`** (port; changes output bytes).

- **Upstream:** `Emitter::WriteStreamable` `include/yaml-cpp/emitter.h:153-188` @ 0.8.0 ->
  `161-197` @ 0.9.0. The non-special branch was `stream << value` and is now
  `stream << FpToString(value, stream.precision())`. The new code is
  `include/yaml-cpp/fptostring.h` and `src/fptostring.cpp:1-255`, on
  `src/contrib/dragonbox.h` (`jkj::dragonbox::to_decimal` with its default policies: return
  the sign, remove trailing zeros, nearest-to-even, to-even, full cache). The stream is also
  imbued with `std::locale::classic()`, which has no effect (see Y15).
- **What changes.** `FpToString(v, p)` takes the shortest round-trip digits of `v` *in its own
  type*: binary32 for `float` and binary64 for `double`. If there are more than `p` digits,
  it truncates them to `p` and rounds up when the next decimal digit is `'5'` or more. It does
  not strip the zeros that truncation leaves. It writes scientific notation when the decimal
  exponent is `>= p` or `<= -5`, with an exponent of at least two digits. `0`, `inf` and `nan`
  fall back to a classic-locale `ostream`, and `inf`/`nan` never reach it because the emitter
  writes `.inf`/`.nan` first. Compared with `%.{p}g` of the exact value, which is 2.5.2's
  output on both CRTs:
  - zeros after truncation are kept: `0.30000000000000004` -> `0.300000000000000`,
    `1.0000000000000002` -> `1.00000000000000`, and the float `1.0000001f` at 7 ->
    `1.000000`. 2.5.2 printed `0.3`, `1` and `1`;
  - double rounding: the 16th digit of the *shortest* string decides, not the exact binary
    value, so a value whose shortest form ends in `...x5` rounds up even when it lies below the
    midpoint;
  - subnormals use their short shortest form: `5e-324`, where `%.15g` gives
    `4.94065645841247e-324`;
  - values whose shortest form has at most `p` digits print as before, for example every
    literal with 15 digits or fewer that a config was read from.
- **Where OCIO uses it.** Every number `Config::serialize()` writes. `OCIOYaml.cpp` sets
  `SetDoublePrecision(15)` and `SetFloatPrecision(7)` (`OCIOYaml.cpp:5444-5445` @ v2.5.2).
  Doubles include matrices, offsets, CDL slope/offset/power/sat, exposure/contrast, log and
  exponent parameters, ranges, `luma`, fixed-function parameters and grading values. Floats are
  `std::vector<float>`: the allocation vars and the grading curve control points and slopes.
  The text feeds `Config::getCacheID` (the hash of `serialize()`) and the `config.ocio` that
  `Config::archive` writes into `.ocioz` archives, whose CRC and sizes follow.
- **Rust:** `crates/ocio/src/yaml_cpp/emitter.rs`, `write_streamable`, `write_f32` and
  `write_f64`. `write_f32` widens to `f64` today; it must keep `f32`, because the shortest
  digits of `0.1f` widened are `0.10000000149011612`, which truncates to `0.1000000`. New:
  `fptostring.rs` and a dragonbox port (`dragonbox.rs`).
- **Implementation choice.** Port dragonbox's `to_decimal` for binary32 and binary64 with its
  default policies, and its full cache tables verbatim (rule 1). The alternative is Rust
  `core`'s shortest formatting (`{:e}`, Grisu with a Dragon4 fallback). It also gives the
  shortest, closest digits and needs no new notice, but it isn't a port, and its agreement on
  every tie and boundary would have to be proved through the wheel at precisions 7 and 15,
  which can't show the digits past 15. **Recommendation: port dragonbox.**
- **Register:** a new I-entry (I-950): written numbers keep the zeros of a truncation and
  round on the shortest digits. The fix is `%.{p}g`, or strip the zeros.

**Y2. The scanner refuses some lines it accepted** (port; changes accepted inputs and error
text).

- **Upstream:** `Scanner::ScanNextToken` `src/scanner.cpp:82-174` @ 0.8.0 -> `83-193` @ 0.9.0,
  with the new lines 130-141 and 174-179. `ScanToNextToken` `176-235` -> `195-257` (new
  225-226). `StartStream`/`EndStream` `245-265` -> `267-289`, and `m_scalarValueAllowed` in
  `src/scanner.h:180`. `Scanner::ScanValue` `src/scantoken.cpp:192-219` -> `192-222` (new
  214-215) and `ScanQuotedScalar` `331-375` -> `334-382` (new 366-369). The new messages are in
  `include/yaml-cpp/exceptions.h:51-52`. yaml-cpp commit 1f2b841.
- **What changes.** A `,` in column 0 is now an error, "plain value cannot start with flow
  indicator character", even inside a flow sequence. After a quoted scalar in block context,
  another scalar or a `,` on the same line is an error, "unexpected scalar": `"foo" "bar"`,
  `"foo", bar`. The flag resets at each line break and at each `:`.
- **OCIO-visible:** yes. Configs with such lines that 2.5.2 loaded now fail with
  `yaml-cpp: error at line L, column C: ...`. OCIO 2.6.0 had to change its own test
  (`tests/cpu/Config_tests.cpp:6539`, `inactive_color_space_read_write`: the `,` of a
  multi-line `inactive_colorspaces` list moved off column 0).
- **Rust:** `crates/ocio/src/yaml_cpp/scanner.rs`, `scan_token.rs` and `exceptions.rs`
  (`error_msg`).

**Y3. Line endings are detected from the first one** (port; changes marks, and so error
lines, and what parses).

- **Upstream:** `Stream::get()` `src/stream.cpp:260-273` @ 0.8.0 -> `260-290` @ 0.9.0, with
  `m_lineEndingSymbol` in `src/stream.h:56`. yaml-cpp commit ee9c4d1.
- **What changes.** 0.8.0 starts a new line, column 0, only at `\n`. 0.9.0 takes the line
  ending from the first `\n` or `\r` it reads: `\r\n` and `\n` both mean `\n`, and a lone `\r`
  means `\r`. It then starts lines only at that character. A file with only `\r` endings has
  real lines and columns now. In 0.8.0 its column never reset, which broke indentation, so
  such configs fail to parse in 2.5.2 and parse in 2.6.0 (upstream test `BlockCREncoded`). A
  file whose first ending is a lone `\r` and whose later ones are `\n` now never resets the
  column at those `\n`.
- **OCIO-visible:** configs with classic-Mac or mixed line endings now load, or fail
  differently, and their error messages carry other line and column numbers.
- **Rust:** `crates/ocio/src/yaml_cpp/stream.rs`, `get`. Register: I-100's citation moves.
  A new I-entry (I-951) covers mixed endings counted by the first one.

### Changes ported for exactness (no OCIO-visible effect)

**Y4. An empty flow sequence doesn't indent before `[`.** `Emitter::EmitEndSeq`
`src/emitter.cpp:204-227` @ 0.8.0 -> `204-228` @ 0.9.0 (commit c67d701): `IndentTo` only for an
originally block group or a begun node. It affects an empty flow sequence written where the
column is below the current indent, such as a map *key* (`[]: 1`). OCIO writes empty flow
sequences only as values after `key: `, where the column is already past the indent: at depth
d the column is at least 2d + len(key) + 2, against an indent of 2d + 2. So OCIO is
unaffected. Rust: `emitter.rs` `emit_end_group`, which serves both sequences and maps. Only
`EmitEndSeq` changed, so it must be split. Upstream test `EmptyFlowSeqInMap`.

**Y5. `as<T>()` of a node without data throws `InvalidNode`.** `as_if<T, void>` and
`as_if<std::string, void>` `include/yaml-cpp/node/impl.h:122-151` @ 0.8.0 -> `122-153` @ 0.9.0
(commit 3d2888c): `InvalidNode(m_invalidKey)` ("invalid node; ...") replaces
`TypedBadConversion` ("bad conversion") for `Node()`, and for an undefined node from the
inserting `operator[]`. OCIO only converts nodes it gets from iteration, or `node[key]` right
after finding `key` (`OCIOMYaml.cpp:323` @ v2.6.0), so it never reaches this. Rust:
`node.rs` `as_`, `convert.rs` `as_if`. The `T t = fallback` initialization (impl.h:100) has no
effect.

**Y6. `LoadAll` stops at the first null document.** `src/parse.cpp:50-63`, line 56 (commit
bc67157). OCIO uses `YAML::Load`. Rust: `parse.rs` `load_all`, which the port's tests use.
Upstream test `InfiniteLoopNodes`.

**Y7. `FpToString`'s overflow fix for large precisions.** `src/fptostring.cpp:132-228`
(`overflow_zeros`, commit c7aa78d) is part of Y1's final code. OCIO's precisions are 15 and 7,
so it never overflows. Port it with Y1 and its upstream test
`vulnerability_stack_buffer_overflow`.

### No effect on the port

| # | Change | Why no effect |
|---|---|---|
| Y8 | `EmitFromEvents::EmitProps` writes `!tag` as `LocalTag` (`src/emitfromevents.cpp:118-123` -> `118-128`) | The port doesn't emit nodes: no `EmitFromEvents` (`yaml_cpp/mod.rs`), and OCIO never does `emitter << node` |
| Y9 | `Write(const char*, size)` and the `string_view` overloads; `ComputeStringFormat`, `Write*String`, `WriteAlias/Anchor/Comment` take a pointer and a size (`emitter.cpp:716-757`, `emitterutils.cpp`) | Same bytes: the old `std::string` paths already used `size()`; `operator<<(const char*)` still uses `strlen` |
| Y10 | `IsNullString(const char*, size_t)` (`src/null.cpp`) | Same comparisons, including strings with a NUL byte (size checked) |
| Y11 | `static const RegEx&` -> `static const RegEx` in `IsValidPlainScalar` | Lifetime only |
| Y12 | `node_data::convert_sequence_to_map`, `streamable_to_string`, `Parser::HandleYamlDirective` and `convert<>` imbue `std::locale::classic()` | The wheels never change C++'s global locale; Python's `setlocale` changes C's, not `std::locale::global` |
| Y13 | `memory_holder::merge` swaps the smaller set into the larger (`src/memory.cpp:12-14`) | Performance only; the node sets are the same |
| Y14 | iterator `reference` typedefs, `EmitterStyle`/`NodeType` as namespaces, `TokenNames` gains `NON_PLAIN_SCALAR`, `<iostream>` removed, `dll.h` pragmas | Compile-time only; `TokenNames` is used only by `operator<<(Token)`, debug output the port doesn't have |
| Y15 | `convert<double>::encode` uses `FpToString` at `max_digits10` (`node/convert.h:133`) | The port's nodes are read-only: no `encode` |

### yaml-cpp 0.9.0 tests to port

Expected values from yaml-cpp's own tests at the wheel's version (CLAUDE.md rule 2):

- `test/fptostring_test.cpp` (new): `conversion_double`, `conversion_float` and
  `vulnerability_stack_buffer_overflow`. Their `convert_with_stringstream` comparisons need the
  platform C runtime as the reference (`ocio_testkit` `crt.rs`: `std::stringstream << v` at
  default precision is `%g`). The port's own `OStringStream` must not stand in for it
  (rule 2).
- `test/integration/emitter_test.cpp`: `NumberPrecision` (two values and the expectation
  changed, against the port's existing 0.8.0 copy), `EmptyFlowSeqInMap`,
  `SimpleStdStringScalar`, and `UnterminatedStdStringViewScalar` (a byte slice in the port).
  **Not portable:** `SimpleStdStringViewScalar`, whose only difference from the string test is
  a C++ `string_view`, and `LocalTagRetainedAfterLoadingNode`, which emits a node.
- `test/integration/load_node_test.cpp`: `LoadNonClosedQuotedString`,
  `LoadWrongQuotedString`, `LoadUnquotedQuotedStrings`, `LoadCommaSeparatedStrings`,
  `InfiniteLoopNodes`, `LoadSameLineStrings` and `BlockCREncoded`. `LoadNewLineStrings`
  emits the loaded node for its non-throwing cases, so only with node emission. Decide in the
  chunk: port its throwing cases under its name, saying so, or leave it unported.
- `test/node/node_test.cpp`: `MissingKey` needs the inserting `operator[]`, and the
  `SimpleFlowSeqNode` and `RobustAgainstLocale` changes are `NodeEmitterTest`s. Neither is
  portable, for the reasons `node_tests.rs` already gives.
- `test/handler_test.h` moved to `test/integration/` with no change: citation paths only.

## expat 2.7.2 -> 2.8.4

OCIO creates parsers with `XML_ParserCreate(NULL)`, sets the start, end and character data
handlers, and reads only `XML_GetErrorCode` and `XML_ErrorString`. Its line numbers are its
own. 2.6.0 adds `XML_StopParser` calls from its handlers (X4). `XML_ErrorString`
(`xmlparse.c:2856-2965` @ R_2_7_2 -> `2867-2976` @ R_2_8_4) and the `XML_ERROR_*` enum are
unchanged byte for byte.

### Changes that reach OCIO

**X1. An empty `version` in the XML declaration is refused** (port; new parse error).
`doParseXmlDecl` `expat/lib/xmltok.c:1152-1230` @ R_2_7_2 -> `1158-1242` @ R_2_8_4, new lines
1184-1190 (#1286). `<?xml version=""?>` now fails `processXmlDecl` with `XML_ERROR_XML_DECL`
("XML declaration not well-formed"). It reaches OCIO through CTF/CLF, CDL/CC/CCC and Iridas
look files with that declaration, which 2.5.2 read.
Rust: `crates/ocio-formats/src/expat/xmltok.rs`, the `doParseXmlDecl` port at line 1168.
Upstream test `test_xmldecl_empty_version`.

**X2. A general entity that ends at another tag level is refused** (port; new parse error).
`internalEntityProcessor` `xmlparse.c:6390-6465` @ R_2_7_2 -> `6511-6595` @ R_2_8_4, new lines
6561-6564 (2.7.3, #1059, commit ba80428c). When a non-parameter entity's text is fully
processed, `openEntity->startTagLevel != m_tagLevel` returns `XML_ERROR_ASYNC_ENTITY`
("asynchronous entity"). This closes a 2.7.0 regression that let documents such as
`<!ENTITY g1 '<t1>&g0;'>` ... `<t0>&g1;</t1></t0>` through. It reaches OCIO through XML
files with an internal DTD whose entities hold tags. Rust: `xmlparse.rs`
`internal_entity_processor`. Upstream tests `test_misc_async_entity_rejected` and
`test_misc_sync_entity_tolerated`.

**X3. The allocation tracker counts other sizes** (port; moves the memory limit's threshold,
I-171). The tracker's arithmetic is unchanged:
`EXPAT_MALLOC_PADDING = sizeof(long long) - sizeof(size_t)` is 0 on x86-64 for both compilers
(`internal.h:153-156` @ R_2_8_4). What it counts changes:

- `ELEMENT_TYPE` (`xmlparse.c:384-391` -> `409-417`): `nDefaultAtts` and `allocDefaultAtts`
  become `size_t`, and the type gains `HASH_TABLE defaultAttForName`. It is 40 bytes today
  (`heap.rs` `SIZEOF_ELEMENT_TYPE`) and 88 by layout. Every distinct element name allocates
  one.
- `TAG_NAME` lengths become `size_t` (`xmlparse.c:286-293` -> `292-299`), so `TAG` grows from
  88 to 96 by layout (`SIZEOF_TAG`).
- `DTD` gains `size_t scaffIndexSize` (`xmlparse.c:420` -> `446`): 360 -> 368 (`SIZEOF_DTD`).
- `struct XML_ParserStruct` (`xmlparse.c:663-785` -> `719-845`). `m_parseEndByteIndex` and
  `POSITION`'s fields (`xmltok.h:149-150`) become `uint64_t`. The three free entity lists
  become one. The struct gains `m_unknownEncodingConvert`, the 16-byte SipHash key with its
  `XML_Bool`, and `unsigned m_handlerCallDepth`. `m_attsSize` and `m_groupSize` become
  `size_t`. Today's sizes are 1,048 bytes (MSVC) and 1,088 (GCC), `heap.rs` `SIZEOF_PARSER`.
  Both must be measured again, and the 40-byte difference between the platforms may change or
  vanish.
- Default attributes get a name table (CVE-2026-45186, CVE-2026-66046 and CVE-2026-76641; 2.8.1
  and 2.8.4). `defineAttribute` (`xmlparse.c:7141-7199` -> `7290-7354`) inserts each declared
  attribute into `type->defaultAttForName`: a 64-slot table (512 bytes) on the first insert,
  doubling when half full, and 24 bytes (`NAME_AND_DEFAULT_ATTRIBUTE`) per name. `dtdReset` and
  `dtdDestroy` free those tables (`7661-7720`). `storeAtts` and `getElementType` look names up
  there instead of scanning (`3862-4320`, `8651-8672`). That gives the same attributes and the
  same `isCdata`, because the first declaration still wins; only the counted bytes differ.
- One free list for open entities (`processEntity` `6329-6388` -> `6452-6509`, and the
  closers in `internalEntityProcessor`, `storeAttributeValue` and `callStoreEntityValue`): a
  freed attribute-entity record can now be reused for a content entity, so fewer records are
  allocated. Today `heap.rs` keeps three lists (`EntityList`).
- `poolAppendChars` and `poolGrowUntil` (`xmlparse.c:696-708`, `8429-8440`) replace the
  char-by-char appends. They make the same `poolGrow` calls in the same order, because
  `poolGrow`'s result depends only on `end - start`, never on `ptr`. **No effect**, but cite
  the new code.

OCIO-visible only for documents within a few hundred bytes of the 64 MiB or 100x limit, and for
documents with many distinct element names or declared attributes, where the threshold moves.
Rust: `crates/ocio-formats/src/expat/heap.rs` and `xmlparse.rs`. Evidence:
`crates/ocio/tests/xml_memory_oracle.rs`, which finds each family's first refused size in both
wheels, re-run against 2.6.0.

**X4. `XML_StopParser` from a handler** (port the non-resumable path; needed by OCIO 2.6.0).
OCIO PR #2350 (`2e1c9ac0`, "Fix expat 2.8 exception handling") wraps the handlers of
`FileFormatCTF.cpp`, `FileFormatIridasLook.cpp` and `cdl/CDLParser.cpp`. A handler now catches
its exception, keeps the first one, and calls `XML_StopParser(parser, XML_FALSE)`, and `Parse`
rethrows it after `XML_Parse` returns. 2.8.2's handler-depth guard (`beforeHandler` and
`afterHandler`, `xmlparse.c:1186-1201`, CVE-2026-50219) makes `XML_ParserFree` refuse a parser
that a C++ exception left inside a handler, hence the change.
`XML_StopParser` (`xmlparse.c:2646-2680` -> `2640-2675`) is not itself guarded. What changes
for OCIO: expat finishes the current token's handler calls before it sees `XML_FINISHED`. For
an empty element it calls the end handler after the start handler failed. Its exception is
dropped, but its side effects (OCIO log messages, element-stack pops) happen. The port
translated no `XML_StopParser`, and a handler's `Err` ends the call at once
(`xmlparse.rs:20-55`; the `// XML_SUSPENDED, XML_FINISHED?` comments at 1982 and 2214 mark the
skipped status checks). Port `XML_StopParser(XML_FALSE)` and the parsing-status checks after
handler calls in `doContent`, `doCdataSection`, `doProlog` and `epilogProcessor`. The reader
side is listed under "Items for other cards".

### No effect on the port

| # | Change | Why no effect |
|---|---|---|
| X5 | Handler-depth guard on `XML_Parse`, `XML_ParseBuffer`, `XML_GetBuffer`, `XML_ParserReset`, `XML_ParserFree` and `XML_ResumeParser` (2.8.2) | OCIO's handlers never call back into the parser; the port's handlers can't reach it |
| X6 | `XML_Parse`/`XML_ParseBuffer` overflow checks in `uint64_t` (`2351`, `2463-2469`) | The port takes the buffered path (`XML_CONTEXT_BYTES` 1024); the new check needs 2^64 bytes |
| X7 | `XML_GetCurrentLineNumber`/`ColumnNumber`/`ByteIndex` cast from 64-bit counters to `XML_Size`/`XML_Index` (32-bit on Windows) | OCIO reports its own line count, never expat's (`xmltok.rs:53-56`) |
| X8 | 16-byte hash salt and entropy sources, `XML_SetHashSalt16Bytes` (2.8.0, 2.8.4 getentropy fix) | Salted hashes order nothing visible; the port's tables are maps (`xmlparse.rs:44-47`) |
| X9 | `*_toUtf16` surrogate fix (`xmltok.c:708-714`, CVE-2026-72522) | Only for `XML_UNICODE` builds; the wheels' `XML_Char` is `char` |
| X10 | Unknown-encoding handler data and re-entry (2.7.4, 2.8.4) | OCIO sets no unknown-encoding handler |
| X11 | External entity parsers, parameter entities, `entityValueProcessor`'s `XML_TOK_INSTANCE_START`, `storeEntityValue`'s empty-text early exit, `dtdCopy`/`copyEntityTable` (2.7.5, 2.8.2) | Not reached: `XML_SetParamEntityParsing` stays `NEVER`, no external entity handler. For internal entities, the empty-text exit gives the result the tokenizer gave (`XML_TOK_NONE`, not accounted) |
| X12 | Content-model scaffold index sizing (`doProlog` `6030-6045`, CVE-2026-56132) and the group connector's undo on failure (2.7.4) | The scaffold is built only with an element declaration handler. The group connector grows by the same sizes |
| X13 | Integer-overflow guards (`storeAtts`, `addBinding`, `getAttributeId`, `textLen`, `copyString`, tag buffer doubling), `int`->`size_t` counters | Reached only past 2^31-byte names or counts; `addBinding`/`BINDING` need namespace processing, which OCIO doesn't enable |

### expat 2.8.4 tests to port

- `tests/basic_tests.c`: `test_xmldecl_empty_version`; the attribute-default tests
  `test_duplicate_cdata_attribute`, `test_duplicate_id_attribute_1` and `_2`,
  `test_duplicate_cdata_attribute_multiple_attlistdecl` (and `_2`, `_3`) and
  `test_duplicate_id_attribute_multiple_attlistdecl`, which cover X3's `defineAttribute` and
  `storeAtts` paths. `test_attributes` changed only its `ElementInfo` initializers
  (`handlers.h` gained a field), so re-check the port's copy against it.
- `tests/misc_tests.c`: `test_misc_async_entity_rejected` and `test_misc_sync_entity_tolerated`
  (X2). `test_misc_version` now expects `expat_2.8.4`, if the port carries it. Check
  `test_misc_input_2gb` (2+ GiB documents, 2.8.3) against the port's existing I-177 test before
  deciding.
- **Not portable** (APIs or builds OCIO doesn't reach): `test_hash_salt_setter`,
  `test_default_attr_index_after_dtd_copy`,
  `test_scaff_index_shared_across_external_entity_parser`, `test_unknown_encoding_user_data_*`,
  `test_empty_ext_param_entity_in_value`, `test_misc_no_infinite_loop_issue_1161` (external
  parameter entities), `test_misc_calls_forbidden_from_handlers`,
  `test_misc_resume_parser_forbidden_from_handler`,
  `test_misc_unknown_encoding_callbacks_protected`, `test_misc_low_surrogate_mozilla_bug_2053153`
  (`XML_UNICODE`), `test_alloc_tracker_pointer_alignment` (the port counts, it doesn't
  allocate) and `test_nsalloc_set_context_zombie` (namespaces).

## minizip-ng 4.0.10 -> 4.2.2

OCIO calls `mz_zip_writer_add_path`, `mz_zip_writer_entry_*`, `mz_zip_reader_open_file`,
`mz_zip_reader_goto_*`, `mz_zip_reader_entry_save_buffer(_length)`, `mz_zip_reader_save_all`,
the memory stream and `mz_os_*_dir`/`mz_path_compare_wc` (`OCIOZArchive.cpp` @ v2.6.0). It
builds minizip-ng with zlib only (`Installminizip-ng.cmake` @ v2.6.0 adds `-DMZ_PPMD=OFF`).

### Changes that reach OCIO

**M1. Extraction refuses symlinks that escape the destination** (port; new refusals in
`ExtractOCIOZArchive`).

- **Upstream:** `mz_zip_reader_save_all` `mz_zip_rw.c:818-907` @ 4.0.10 -> `848-941` @ 4.2.2
  stores `reader->destination_dir`. `mz_zip_reader_entry_save_file` `647-775` -> `665-805`
  calls `mz_dir_has_unsafe_symlink(directory, destination_dir)` before the overwrite check
  (new lines 700-704). That walks the entry's directory component by component and refuses
  (`MZ_EXIST_ERROR`) a component below the destination that is a symlink whose target resolves
  outside it. For a symlink entry (both the `linkname` and the stored-target paths, lines
  729-735 and 752-760) it calls `mz_path_is_symlink_target_safe(path, target, destination)`,
  which refuses absolute targets and targets that resolve outside. Both are new in
  `mz_os.c:280-421`. `mz_os_read_symlink` (`mz_os_posix.c:332-341` -> `357-371`) now returns
  `MZ_BUF_ERROR` when the target fills the buffer. Commits bb282ac, 5a3c62c, 185667e, 9963dd3,
  32747dc, 0574b3b, 483f056 and 131d59b.
- **OCIO-visible:** archives with symlink entries, which OCIO never writes but extracts, and
  destinations that already hold links pointing out. `ExtractOCIOZArchive` now fails on them
  where 2.5.2 extracted. Ordinary archives into ordinary directories are unaffected: every
  component check finds no link.
- **Rust:** `crates/ocio/src/minizip/mz_zip_rw.rs` (`save_all`, `entry_save_file`) and
  `mz_os.rs` (new `mz_dir_has_unsafe_symlink`, `mz_path_is_symlink_target_safe`, and a
  `mz_os_read_symlink` with the buffer check). `mz_path_resolve` and `mz_path_append_slash`
  exist already. Windows symlink functions are unchanged. Upstream test: `test/test_path.cc`
  `symlink_target_base` (new).

**M2. Files extracted on Linux are created `0640`** (port; Linux file modes).
`mz_stream_os_open` `mz_strm_os_posix.c:67-93` -> `71-110`: `open(path, flags, S_IRUSR |
S_IWUSR | S_IRGRP)` and `fdopen` replace `fopen64(path, "wb")`, which created files `0666`.
Both are masked by the umask. `mz_zip_reader_entry_save_file` then applies the entry's
attributes when `mz_zip_attrib_convert` can convert them (MS-DOS/Windows and Unix made-by
systems), so the mode differs from 2.5.2 only for entries whose made-by system it can't
convert. Rust: `mz_strm.rs` `mz_stream_os_open` (create mode via `OpenOptionsExt::mode`).
`O_NOFOLLOW` comes only with `MZ_OPEN_MODE_NOFOLLOW`, which only a link-storing writer sets.
Windows is unchanged (`mz_strm_os_win32.c` didn't change).

**M3. The writer's link handling moved into `mz_zip_writer_add_file`** (port for exactness;
no OCIO-visible effect). `mz_zip_rw.c:1570-1647` -> `1630-1721`: a symlink is skipped when
neither storing nor following links. Its attributes come from `mz_os_get_link_attribs` only
when storing links, and the new `mz_os_get_link_attribs` takes the `lstat` that
`mz_os_get_file_attribs` gave up for `stat` (`mz_os_posix.c:243-252` -> `253-262`, `336-345`;
Windows `301-318` -> `333-373` with the reparse-point query, `543-560`; commit 6b9defc).
OCIO's `add_path` already returns before `add_file` for a link, whether to a file or a
directory (`mz_zip_rw.c:1686-1689` @ 4.0.10 -> `1762-1765` @ 4.2.2, unchanged; I-211). For the regular
files it does add, `stat` and `lstat` agree. So the written archives are unchanged.

### No effect on the port

| # | Change | Why no effect |
|---|---|---|
| M4 | `mz_zip_reader_entry_save_buffer` opens its memory stream `WRITE` instead of `READ` (`807-832`, commit 3ad5bb4) | `mz_stream_mem` only looks at `MZ_OPEN_MODE_CREATE`; neither mode has it |
| M5 | `mz_zip_reader_set_pattern` keeps the caller's pointer (`945-950`, revert of #856) | OCIO sets no pattern |
| M6 | `mz_stream_mem_seek` refuses positions past `INT32_MAX` (`mz_strm_mem.c:167-171`) | The memory stream's size is an `int32_t` anyway; OCIO's archives never get near it |
| M7 | `mz_stream_split_write` counts in 64 bits (`mz_strm_split.c:258-284`) | OCIO never writes split archives |
| M8 | `mz_zip_recover_cd` checks `mz_zip_get_cd_mem_stream` (`mz_zip.c:1261-1266`) | Fails only when an allocation fails |
| M9 | PPMd support (`mz_strm_ppmd.c`, method 98 in `mz_zip.c`) | `-DMZ_PPMD=OFF`; I-208's "other methods read as zeros" stays |
| M10 | `MZ_VERSION_BUILD` 0x04000A -> 0x040202; OCIO's `#if MZ_VERSION_BUILD >= 0x040000` fix (`OCIOZArchive.cpp`, `ocioarchive/main.cpp`) | The old octal `040000` (16384) was also below both versions: same branch |
| M11 | `mz_os_path_same_fs`, `mz_os_get_temp_path`, `MoveFileExW` with `MOVEFILE_COPY_ALLOWED` | Only for the minizip tool's in-place erase; OCIO never renames |
| M12 | `mz_os_utf8_string_create` scope fix (POSIX, `mz_os_posix.c:47-62`) | Built with `MZ_ICONV=OFF` |
| M13 | DOS-date comments, `mz_zip_tm_to_time_t` doc | Comments only |
| M14 | NULL-handle checks and typed handles throughout `mz_zip_rw.c`/`mz_zip.c`/`mz_strm.c` | OCIO never passes NULL; same calls otherwise |

### minizip-ng 4.2.2 tests to port

`test/test_path.cc` `symlink_target_base` (M1). Its seven cases run with `/` on Linux and `\`
on Windows, as upstream converts them. The other test changes (`test_compat.cc`,
`test_crypt.cc`, `test_stream_crypt.cc`, `test_stream_compress.cc` ppmd) cover code OCIO's
build leaves out.

## Imath 3.2.1 -> 3.2.3

| # | Change | Why no effect |
|---|---|---|
| I1 | 3.2.2: CMake and comment fixes. 3.2.3: `is_float_like` SFINAE for `Vec` methods, install paths, a C++23 deprecation warning in `numeric_limits<half>`, `imath_float_half_exp_table` moved under `IMATH_USE_HALF_LOOKUP_TABLE` (`CHANGES.md`: "No functional differences") | `imath_half_to_float` and `imath_float_to_half` (`src/Imath/half.h:284-364` and `373-443` @ 3.2.3) are the port's code line for line, at the line numbers the port cites for 3.2.1. So are `half::mantissa`/`exponent` (`802-811`) and `isNan`/`isInfinity` (`838-847`). The build choices the port relies on are unchanged: OCIO 2.6.0's `InstallImath.cmake` and compiler flags don't change, so there is no lookup table and no `-mf16c` for library sources. Only the version labels and citations move. |

No Imath tests to port: the port checks half conversions exhaustively against the wheel and
the CPU (`crates/ocio-ops/tests/half_conversions.rs`, spike S4) and ports no ImathTest files.

## zlib 1.3.1 -> 1.3.2

The port doesn't translate zlib. Deflate and inflate are `flate2` on `zlib-rs` (owner decision
2026-10-17, deviation D-6), and its deflate bytes are zlib's for entries up to a few KiB and
differ beyond. `mz_strm_zlib.c` is unchanged between minizip-ng 4.0.10 and 4.2.2. It calls
`deflateInit2(level 9 from OCIO's BEST, Z_DEFLATED, -15, DEF_MEM_LEVEL 8,
Z_DEFAULT_STRATEGY)` and `inflateInit2(-15)`.

| # | Change (ChangeLog 1.3.1.2 and 1.3.2) | Effect |
|---|---|---|
| Z1 | Deflate: `deflateBound()` more conservative, `deflateUsed()`, `deflateCopy` copies only initialized state, NULL checks, `_z` size variants. Inflate: state zeroed on allocation, fixed tables built once with atomics, `inflateCopy` fixes, `inflatePrime` for 16-bit `int` | **No change expected** in deflate output or inflate results: none of these touches the match finder, lazy evaluation, block splitting or Huffman coding that decide deflate's bytes at level 9, nor inflate's decoding. minizip-ng calls neither `deflateBound` nor the copy functions. Without the 1.3.1 sources this rests on the ChangeLog, and the probe below settles it. If the bytes are unchanged, D-6's measurement and `zlib-rs` 0.6.8 stay as they are. |

**Evidence that settles Z1 (after phase 0):** archive the same configs and LUT files (random
bytes and LUT text of 1, 4, 8, 64 and 512 KiB) through the 2.6.0 wheel and compare each entry's
compressed bytes with the 2.5.2 wheel's, which are kept in the oracle cache or re-run from the
`v1.0.0` tag's oracle. They should be byte-identical. Also check that the inflate-boundary
cases of I-208 (`ocioz_read_oracle.rs`, `inflate_at_read_boundaries`) still agree. Confirm
the version in the binaries by their `" deflate 1.3.2 Copyright 1995-2026 ..."` and
`" inflate 1.3.2 ..."` strings. If the bytes do differ, D-6's "zlib's bytes for small
entries" claim must be measured again against 1.3.2, and the owner decides whether
`zlib-rs` needs a change.

## THIRD-PARTY-NOTICES.md changes

Each library's chunk updates its own sections, in `THIRD-PARTY-NOTICES.md` and in the crate
copy that `cargo xtask guards` checks:

| Section | Copies | Change |
|---|---|---|
| `## Imath 3.2.1` | root, `crates/ocio-ops/THIRD-PARTY-NOTICES.md` | -> 3.2.3, "OpenColorIO 2.6.0". `LICENSE.md` @ 3.2.3 starts "Copyright Contributors to the OpenEXR Project.", BSD-3-Clause as quoted: no text change |
| `## yaml-cpp 0.8.0` | root, `crates/ocio/THIRD-PARTY-NOTICES.md` | -> 0.9.0, tag `yaml-cpp-0.9.0` (tags use the `yaml-cpp-` prefix again), commit 56e3bb550c91fd7005566f19c079cb7a503223cf. `LICENSE` unchanged. `fptostring.*` carry no notice of their own since commit 06ffaf3 |
| **new**: dragonbox (if Y1 ports it) | root, `crates/ocio/THIRD-PARTY-NOTICES.md` | `src/contrib/dragonbox.h` @ yaml-cpp 0.9.0: "SPDX-FileCopyrightText: 2020-2024 Junekey Jeon", `Apache-2.0 WITH LLVM-exception` or `BSL-1.0`, plus MIT "for usage in yaml-cpp". **Owner question:** which licence to cite. Recommendation: Apache-2.0 WITH LLVM-exception, whose full text this file already reproduces for the MSVC STL |
| `## expat 2.7.2` | root, `crates/ocio-formats/THIRD-PARTY-NOTICES.md`, the header of `crates/ocio-formats/src/expat/mod.rs` | -> 2.8.4, tag `R_2_8_4`, commit 12cf0b1f25f026a022fe728ad8f7e3d017285b80. The root `COPYING` now reads "Copyright (c) 2001-2026 Expat maintainers" (it said 2001-2025); `expat/COPYING` still says 2025. Quote the root file, as the other sections quote the project's top-level licence |
| `## minizip-ng 4.0.10` | root, `crates/ocio/THIRD-PARTY-NOTICES.md` | -> 4.2.2, commit 7b2387161c542fa9f427352dcdef76097d0d692b, and `FindExtPackages.cmake`'s new line numbers. `LICENSE` unchanged |
| zlib | none | Not translated. The crate table keeps `zlib-rs 0.6.8` unless Z1 says otherwise |

## Register, deviation and citation updates

- `docs/improvements.md`: citations to yaml-cpp 0.8.0 in I-100 to I-105 and I-157; to expat
  2.7.2 in I-170, I-171 (rewrite its numbers after X3) and I-177; to minizip-ng 4.0.10 in
  I-207 to I-214, U-78 and U-80. Check I-213's and I-214's wheel addresses (they name
  `OpenColorIO_2_5.dll`) against the 2.6.0 binaries. New entries from this card's ranges:
  I-950 (Y1), I-951 (Y3). U-730..U-749 are unused so far: no new undefined behaviour reaches
  OCIO.
- `docs/deviations.md` D-6: "minizip-ng 4.0.10" and "zlib 1.3.1" -> 4.2.2 and 1.3.2, after Z1.
- Module headers: `yaml_cpp/mod.rs`, `expat/mod.rs`, `minizip/mod.rs` and `minizip/inflate.rs`
  carry the old versions and the wheels' version evidence. Each chunk moves its library's
  `Port of` citations to the new tag, including the functions whose code didn't change.
- `upstream-map.toml` tracks OpenColorIO's files only: no entry for the libraries.

## Items for other cards

- **OCIO #2350's XML reader trampolines** (`FileFormatCTF.cpp`, `FileFormatIridasLook.cpp`,
  `cdl/CDLParser.cpp` @ v2.6.0). These are OCIO code, but they exist because of expat 2.8 and
  need X4. Proposed here as chunk 6 below. Phase 5 (`ocio26-clf`) edits the same files, so the
  orchestrator should decide who owns it, and order it before the CLF chunks.
- **`Config_tests.cpp` `inactive_color_space_read_write`** (line 6539 @ v2.6.0). Its input
  changed because of Y2. The port's copy has to take the 2.6.0 text in the same chunk as Y2,
  or the test goes red.
- **Red list scheduling.** Once the oracle answers as 2.6.0, every oracle test that serializes
  a value whose shortest form has more than 15 significant digits (computed matrices, inverses,
  products) will differ until Y1 lands. So will XML memory thresholds until X3 lands, and any
  probe hitting Y2, Y3, X1, X2 or M1. Phase 1 (`ocio26-base`) can't clear those itself. The
  yaml-cpp chunks, at least, should land in the same stack as phase 1, or phase 1's red list
  should leave them out by name.

## Evidence after phase 0

1. **Bundled versions.** Linux wheel: `expat_2.8.4` (`XML_ExpatVersion`), yaml-cpp 0.9.0's new
   literals ("plain value cannot start with flow indicator character", "unexpected scalar"),
   `" deflate 1.3.2 "`. Windows DLL: the same literals, since expat's version string isn't
   kept there (`expat/mod.rs`). For minizip-ng, the 4.2.2 code paths: `mz_dir_has_unsafe_symlink`
   is inlined into or called from `mz_zip_reader_entry_save_file` (`tools/wheel-inspect`).
2. **Y1 probes** (oracle, both platforms): serialize configs holding doubles
   `0.30000000000000004`, `1.0000000000000002`, `5e-324`, `2.2250738585072014e-308`, values
   whose shortest form ends in `5` at digit 16, and random doubles. Use a FixedFunctionTransform's
   `params` (any length) or matrices. Do the same with floats through allocation vars at
   precision 7 (`1.0000001f`, `16777216f`, random floats). A large random batch (millions of
   values per tier) goes through the battery's tiers. The exhaustive float tier is feasible:
   2^32 values in about 4,096 configs of 2^20 allocation vars.
3. **Y2/Y3 probes:** the upstream load_node cases inside a config, CR-only and mixed-ending
   configs, and the error text and marks.
4. **X3:** `xml_memory_oracle.rs` against 2.6.0, and `sizeof(struct XML_ParserStruct)` from the
   immediates in each wheel's `parserCreate`, `malloc(sizeof(size_t) + sizeof(...))`
   (`tools/wheel-inspect`), cross-checked with `sizeof` builds of the 2.8.4 sources as
   `heap.rs` documents.
5. **X1/X2/X4:** CTF, CDL and look files with `version=""`, async entities, and a bad
   attribute on an empty element (the end-handler-after-failed-start case), with the wheel's
   log captured.
6. **M1/M2** (Linux, and Windows where links can be made): archives with symlink entries
   inside and outside the destination, destinations with an escaping link, and the extracted
   files' modes for entries with an unconvertible made-by system.
7. **Z1:** as described above.

## Proposed chunks

Ordered by risk, yaml-cpp first. The estimates are non-test lines, with tables counted
separately.

| # | Chunk | Content | Estimate | Gate |
|---|---|---|---|---|
| 1 | `ocio26-deps: dragonbox to_decimal` | Port of dragonbox's `to_decimal` for binary32 and binary64 with yaml-cpp's default policies (`compute_nearest`, the multiplication traits, `remove_trailing_zeros`, the log and division helpers), and its full cache tables verbatim. The new notice section | ~450 code + ~700 table data | `--release --rocky` (numeric) |
| 2 | `ocio26-deps: yaml-cpp 0.9.0 emitter` | Y1 `FpToString` and `WriteStreamable` wiring with `f32` kept, Y4 the `EmitEndSeq` split, Y7. Upstream tests `fptostring_test.cpp` and the emitter tests. I-950. yaml-cpp notice and citations of the emitter files | ~250 | `--release --rocky` |
| 3 | `ocio26-deps: yaml-cpp 0.9.0 parser` | Y2 scanner, Y3 stream line endings, Y5 `InvalidNode`, Y6 `LoadAll`. The load_node tests, the 2.6.0 input of `Config_tests` `inactive_color_space_read_write`, I-951, citations of the parser and node files | ~120 | `--rocky` |
| 4 | `ocio26-deps: expat 2.8.4 parser` | X1, X2, X4 (`XML_StopParser(XML_FALSE)` and the status checks), `defineAttribute`/`storeAtts` through `defaultAttForName` (behaviour only). The basic and misc tests listed above. expat notice, `mod.rs` header and citations | ~250 | `--rocky` |
| 5 | `ocio26-deps: expat 2.8.4 allocation tracker` | X3: the structure sizes per platform, the default-attribute tables, one free entity list, `poolGrowUntil`'s citation. I-171 rewritten. `xml_memory_oracle.rs` against 2.6.0 | ~150 | `--release --rocky` (platform sizes) |
| 6 | `ocio26-deps: XML readers stop the parser` (cross-card, see above) | OCIO #2350's trampolines in the CTF, Iridas look and CDL readers on chunk 4's `stop_parser`: first error kept, the remaining handler calls of the token made. Oracle probes of item 5 above | ~150 | `--rocky` |
| 7 | `ocio26-deps: minizip-ng 4.2.2` | M1 symlink safety on extraction, M2 the Linux create mode, M3 the `add_file`/`get_link_attribs` split. `test_path.cc` `symlink_target_base`. minizip-ng notice, `mod.rs` and citations to 4.2.2, I-207..I-214, U-78 and U-80 citations | ~200 | `--rocky` (Linux modes and links) |
| 8 | `ocio26-deps: Imath 3.2.3 and zlib 1.3.2 bookkeeping` | Version labels and citations in `imath_half.rs`, `half_conversions.rs` and the Imath notice. D-6 and `minizip/mod.rs`/`inflate.rs` for zlib 1.3.2, after the Z1 probe agrees | ~20 (docs and comments) | `guards` + the half tests |

Chunk 1 exceeds the usual ~600 lines because of its data tables, which can't be split from the
code that indexes them without a chunk that is dead on its own. The orchestrator decides
whether to allow it or split the binary32 and binary64 paths, which share the helpers. Chunks 1
and 2 can merge if Rust `core`'s shortest formatting is chosen instead of the dragonbox port
(owner question above).
