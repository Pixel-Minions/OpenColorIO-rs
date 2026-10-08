# Phase 4 cards: file formats

**Status: planned, 2026-10-07.** The owner decided P4-1 to P4-9 on 2026-10-07, all as recommended. Phase 4 starts once its Phase 3
prerequisites land (the context's file resolution, the `FileTransform` class, the config
loaders); parts of it can start earlier, as "Order and parallelism" says.

**Goal: milestone M2** (PLAN.md §10, Phase 4, ~18 pw): every file OCIO 2.5.2 reads, writes or
bakes, byte-exact with the `opencolorio==2.5.2` wheel:
- the format registry and `FileTransform`: upstream's probing order, the file cache, `cccid`,
  the interpolation and direction rules, references between CTF files;
- 19 readers covering 24 format names: spi1d, spi3d, spimtx, Iridas cube, itx and look,
  Resolve cube, flame and lustre (3DL), cinespace (CSP), Houdini, Discreet 1DL, Truelight,
  Pandora (mga, m3d), Nuke `.vf`, ICC (icc, icm, pf), CDL, CC, CCC, CLF and CTF;
- 5 writers (CLF, CTF, CDL, CC, CCC: `GroupTransform::write`) and 12 bakers (`Baker`);
- `.ocioz` archives: reading configs and LUTs from them, `Config::archive`,
  `ExtractOCIOZArchive`;
- the Phase 2 and 3 upstream tests that read files.

"Byte-exact" means:
- **read:** the ops a file gives (their data, cache IDs and the processor's pixels on CPU and
  GPU), every exception and warning text (with line numbers), on both platforms;
- **written and baked text:** byte for byte, per platform where the C runtime or the CPU
  makes the bytes differ (D12);
- **archives:** the entries and their contents (P4-3).

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank lines at v2.5.2 without
`//` comments (block comments counted: the counts are upper bounds). Paths are relative to
`src/OpenColorIO/` unless they start with `tests/` or `ext/`.

## How this phase works

Everything in `docs/cards/phase3.md` → "How this phase works" still applies: cards and PRs, the
chunk gate, oracle-first, owner items. What changes in Phase 4:

- **Platform-sensitive by default.** Readers parse numbers (`NumberUtils::from_chars`,
  `std::istream >> int`, `sscanf`/`sscanf_s`), open files in text mode, and the XML readers
  report expat's texts; writers and bakers format through iostreams. Every reader, writer and
  baker chunk runs `cargo xtask gate --release --rocky`.
- **Files are inputs; the oracle writes them.** Tests build their LUT files in a temporary
  directory (never under `tests/` or `fixtures/`), and the oracle writes the same bytes into its
  own (O4.2). Upstream's `tests/data/files` (258 files, BSD-3) are read in place on both sides.
- **Three checks per reader.** For each file, against the wheel:
  1. the ops: the processor's and the optimized processor's op lists, written out with every
     getter, and their cache IDs (`processor_ops` through O4.2);
  2. the pixels: CPU through the battery or `image_apply`, GPU through `gpu_shader` (both
     through O4.2);
  3. the refusals: every exception's type (`Exception` or `ExceptionMissingFile`) and text,
     and every warning, byte for byte.
- **Generated files.** Each reader gets a generator of valid files (sizes, domains, comments,
  blank lines, CRLF, trailing data) and of broken ones (each check the reader makes, plus
  truncation and garbage at each line), batched (`Oracle::batch`).
- **CPU-dependent text.** Bakers sample a processor on the CPU, and the fast inverses and
  composed LUTs that some readers build run the CPU kernels (`docs/cards/phase2.md`). Baked
  text, and the ops of those files, depend on the CPU: their tests go in the `cpu-tests` alias,
  run under SDE, and are never committed as fixtures.
- **Strings and paths are bytes** (`docs/architecture.md`). File names and `cccid`s are bytes;
  paths go through `Platform::filenameToUTF` and pystring's `os.path` as in Phase 3.
- **Owner items** stop at the owner, labelled `oracle`, `waiver`, `deviation`, `api` or
  `dependency`, as in Phase 3.

**What Phase 4 needs from Phases 2, 3 and 5.**
- Phase 2: the op data every reader builds (Matrix, Range, Log, Gamma/Exponent, CDL, Lut1D,
  Lut3D, FixedFunction, GradingRGBCurve) with their inverses, compositions and fast inverses.
  `GenerateLinearScaleLut1D` (`ops/lut1d/Lut1DOp.cpp:209-228`) comes here, with its callers.
- Phase 3: `FileTransform`'s class (3.1c, landed), its `CollectContextVariables` (3.2a), the
  context's `resolveFileLocation` and the `ExceptionMissingFile` texts (3.5f, landed),
  `ConfigIOProxy` (3.10c), `Config::CreateFromFile`'s `PK` signature (3.10b), `num_get` for
  `istream >> int` (3.3f), the config loaders for the config tests (3.3l–3.3m), and
  `Config::isArchivable` (3.10c).
- Phase 5: the op data of ExposureContrast, GradingPrimary, GradingTone and GradingHueCurve,
  which the CTF reader and writer read and write (P4-2).

---

## WP 4.0: parsing primitives (`ocio-ops`, `ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.0a | `ParseUtils.cpp`: `ConvertSpecialCharToXmlToken`, `ConvertXmlTokenToSpecialChar`, `FloatToString`, `FloatVecToString`, `DoubleToString`, `DoubleVecToString`, `StringToFloat`, `StringToInt` (`istringstream >> int`, through `ocio-ops/src/utils/num_get.rs`), `StringVecToFloatVec`, `StringVecToIntVec`, `nextline` (~180) | `ocio-ops/src/parse_utils.rs` | `ParseUtils_tests.cpp` `xml_text`, `string_to_int`, `string_to_float`, `float_double`, `string_vec_to_int_vec` |
| 4.0b | The C runtime's `sscanf` (glibc, Linux) and `sscanf_s` (UCRT, Windows) for the conversions the readers use: `%d`, `%Ns`, `%c`, `%*s`, literal text and whitespace, the return count; `std::istream >> std::string`, `std::getline`, `istream::getline(buf, n)` with their fail and EOF states (~250) | `ocio-ops/src/utils/cscan.rs` | the platform's C runtime (`ocio-testkit` `crt.rs`, T4.1), generated formats and inputs |
| 4.0c | Text-mode `std::ifstream` on Windows (MSVC `filebuf` in text mode: `CR LF` read as `LF`; a lone `CR` kept; what `0x1A` does), binary mode, and the `std::stringstream` a `ConfigIOProxy` gives; `fileformats/FileFormatUtils.cpp/.h` (66): `HandleLUT1D`, `HandleLUT3D`, `LogWarningInterpolationNotUsed`; `GenerateLinearScaleLut1D` | `ocio/src/fileformats/file_format_utils.rs`, `ocio/src/fileformats/input_stream.rs`, `ocio-ops/src/ops/lut1d/lut1d_op.rs` | T4.1 for text mode; the three LUT tests of `Lut1DOp_tests.cpp` that need no file |

- 4.0b ports C library behaviour from the platform's documented semantics and checks it
  against the C runtime (`CLAUDE.md` rule 2); it translates no glibc source (D3's rule).
- `%d` of a number out of `int`'s range is undefined behaviour in C. Where UCRT and glibc
  agree, the port matches them (`I-` entry); where they differ, it matches each (D12) and the
  entry says so; where either crashes, the port returns an error (`U-` entry).

## WP 4.1: the format registry and `FileTransform` loading (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.1a | `transforms/FileTransform.cpp:319-594` (~230): `FormatRegistry` (the 19 formats registered in upstream's order, each reader "not ported yet" until its chunk), `getFileFormatForExtension`, capability queries, `FileFormat::getName`, the default `bake` and `write` refusals; `FileTransform::GetNumFormats`, `GetFormatNameByIndex`, `GetFormatExtensionByIndex`, `IsFormatExtensionSupported` | `transforms/file_transform.rs`, `fileformats/format_registry.rs` | `FileTransform_tests.cpp` `all_formats`, `format_by_index`, `is_format_extension_supported`; oracle `file_formats` (O4.1) |
| 4.1b | `FileTransform.cpp:186-223, 595-888` (~280): `getLutData` (the `ConfigIOProxy` and the file system), `LoadFileUncached` (the formats of the extension first, then all others; the first success wins; the combined error text), `GetCachedFileAndFormat` (the cache by absolute path and file hash), `ClearFileTransformCaches`, `ClearAllCaches`' file caches | `transforms/file_transform.rs`, `caching.rs` | `FileTransform_tests.cpp` `load_file_ok`, `load_file_fail`; `Caching_tests.cpp` `generic_cache` |
| 4.1c | `FileTransform.cpp:890-972` (~70): `BuildFileTransformOps` (direction, `cccid`, CDL style, interpolation), the processor metadata's files, `ReferenceOpData` resolution (`ops/reference/ReferenceOpData.cpp/.h`, 121: paths, aliases, cycles) | `transforms/file_transform.rs`, `ocio-ops/src/ops/reference/` | `FileTransform_tests.cpp` `interpolation_validity`, `context_variables`, `cc_file_with_different_file_extension`; `ReferenceOpData_tests.cpp` (12) once CTF reads (4.5h) |

- The registry lists all 24 names from 4.1a, so `GetNumFormats` and the probing order are
  right from the start. A format whose reader isn't ported throws "not ported yet", which the
  loader treats as a failed read, so a file can load through the wrong format until every
  reader lands. The parity sweep (`p4-m2-parity`) runs only then.

## WP 4.2: spi and cube text formats (`ocio`)

| Chunk | Upstream (`fileformats/`) | Rust (`ocio/src/fileformats/`) | Port tests |
|---|---|---|---|
| 4.2a | `FileFormatSpi1D.cpp` (368): `sscanf` header, `%63s` tokens, half domain, components 1–3; `FileFormatSpiMtx.cpp` (130) | `file_format_spi1d.rs`, `file_format_spimtx.rs` | `FileFormatSpi1D_tests.cpp` (6), `FileFormatSpiMtx_tests.cpp` (4) |
| 4.2b | `FileFormatSpi3D.cpp` (290) | `file_format_spi3d.rs` | `FileFormatSpi3D_tests.cpp` (5) |
| 4.2c | `FileFormatIridasCube.cpp` (488): `sscanf`/`sscanf_s` of every line, domains, 1D and 3D | `file_format_iridas_cube.rs` | `FileFormatIridasCube_tests.cpp` (6) |
| 4.2d | `FileFormatResolveCube.cpp` (652): 1D shaper and 3D, ranges, `StringToFloat` | `file_format_resolve_cube.rs` | `FileFormatResolveCube_tests.cpp` (11) |
| 4.2e | `FileFormatIridasItx.cpp` (268) | `file_format_iridas_itx.rs` | `FileFormatIridasItx_tests.cpp` (2) |

- Each chunk ports the format's reader and `buildFileOps`; its `bake` comes with WP 4.8. The
  tests that bake land there.
- `.cube` tries Iridas, then Resolve: 4.2d checks that probing order on files each reader
  refuses.

## WP 4.3: the other line formats (`ocio`)

| Chunk | Upstream (`fileformats/`) | Rust | Port tests |
|---|---|---|---|
| 4.3a | `FileFormat3DL.cpp` (462): flame and lustre, integer tables, bit-depth detection | `file_format_3dl.rs` | `FileFormat3DL_tests.cpp` (5) |
| 4.3b | `FileFormatCSP.cpp` (738): prelut, 1D and 3D, metadata | `file_format_csp.rs` | `FileFormatCSP_tests.cpp` (8) |
| 4.3c | `FileFormatHDL.cpp` (676): `istream >> word`, the sections, 1D, 3D, 3D+1D | `file_format_hdl.rs` | `FileFormatHDL_tests.cpp` (8) |
| 4.3d | `FileFormatDiscreet1DL.cpp` (616): `sscanf`/`sscanf_s` header, integer and half tables, bit depths | `file_format_discreet1dl.rs` | `FileFormatDiscreet1DL_tests.cpp` (6) |
| 4.3e | `FileFormatTruelight.cpp` (322), `FileFormatPandora.cpp` (272), `FileFormatVF.cpp` (269) | `file_format_truelight.rs`, `file_format_pandora.rs`, `file_format_vf.rs` | `FileFormatTruelight_tests.cpp` (3), `FileFormatPandora_tests.cpp` (3), `FileFormatVF_tests.cpp` (3) |

- `.lut` tries Discreet 1DL, then Houdini (`FileFormatHDL.cpp:298`): checked in 4.3c.

## WP 4.4: XML, CDL files and Iridas look (`ocio`)

**The XML parser** (owner item P4-1). OCIO's three XML readers (CTF/CLF, CDL/CC/CCC, Iridas
look) feed expat one line at a time (`FileFormatCTF.cpp:195-212`), react to its start, end and
character-data callbacks (whose splitting decides which text an element sees), and put
`XML_ErrorString(XML_GetErrorCode())` and their own line count into their messages. Which line an
error surfaces on, and its text, are expat's.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.4a | expat (version pinned in this chunk, P4-1): the tokenizer (`xmltok.c`, `xmltok_impl.c`): UTF-8, UTF-16, ISO-8859-1 and US-ASCII input, the BOM, the encoding declaration, every token and its errors (~1,800) | `ocio-formats/src/expat/xmltok.rs`, `xmltok_impl.rs`, `tables.rs` (in `ocio-formats`, which hosts the XML readers; coordinator 2026-10-08) | expat's own tests that reach the tokenizer (`tests/basic_tests.c`, copied as rule 2 allows); oracle through O4.2: malformed CDL/CTF files, their texts and lines |
| 4.4b | expat's prolog state machine (`xmlrole.c`) and the parser (`xmlparse.c`): incremental `XML_Parse` across buffers, element and attribute handling, character data and its callbacks' splitting, CDATA, comments, processing instructions, the DTD and internal entities as the wheels' build has them (`XML_DTD`, `XML_GE`), the amplification limits, `XML_ErrorString` (~2,500 of the parts OCIO reaches) | `ocio-formats/src/expat/xmlrole.rs`, `xmlparse.rs` | as 4.4a |
| 4.4c | `fileformats/xmlutils/XMLReaderHelper.cpp/.h` (607), `XMLReaderUtils.cpp/.h` (246): the element stack, plain, dummy and description elements, `ParseNumber`, `GetNumbers`, `FindSubString`, `Trim`; `XMLWriterUtils.cpp/.h` (181): `XmlFormatter`, `XmlElementWriter`, the escaping | `fileformats/xmlutils/` | `XMLReaderUtils_tests.cpp` (6) |
| 4.4d | `fileformats/cdl/CDLParser.cpp/.h` (842), `CDLReaderHelper.cpp/.h` (234): the CDL, CC and CCC elements, SOP and Sat nodes, descriptions | `fileformats/cdl/` | `FileFormatCC_tests.cpp` read tests (5) |
| 4.4e | `FileFormatCDL.cpp` (228), `FileFormatCC.cpp` (134), `FileFormatCCC.cpp` (197): readers, `cccid` by id or index; `cdl/CDLWriter.cpp/.h` (116) and the three `write`s; `transforms/CDLTransform.cpp:34-118`: `GetCDL`, `CreateFromFile`, `CreateGroupFromFile` (through the file cache, with an empty config); `GroupTransform::write` (`GroupTransform.cpp:114-139`, moved from 4.6e: the write tests need it) | `fileformats/file_format_cdl.rs`, `file_format_cc.rs`, `file_format_ccc.rs`, `cdl/cdl_writer.rs`, `transforms/cdl_transform.rs`, `transforms/group_transform.rs` | `FileFormatCDL_tests.cpp` (2), `FileFormatCCC_tests.cpp` (2), `FileFormatCC_tests.cpp` `test_cc2_load_save`; `CDLTransform_tests.cpp` `create_from_cc_file`, `create_from_ccc_file`, `create_from_cdl_file`, `escape_xml`, `clear_caches` (`faulty_file_content` moves to 4.5h: its last case reads a CTF file) |
| 4.4f | `FileFormatIridasLook.cpp` (487): the look's XML, its hex-ASCII floats, the 3D LUT | `fileformats/file_format_iridas_look.rs` | `FileFormatIridasLook_tests.cpp` (4) |

## WP 4.5: the CLF/CTF reader (`ocio`)

CTF 1.2 to 2.5 and CLF up to 3.0 (CLF 2.0 and older read with CTF 1.7 rules). `xmlns` is
ignored in 2.5.2; SMPTE ST 2136-1 detection is 2.6 and later. Element names compare ignoring
case; namespace prefixes are stripped, except inside `<Info>`.

| Chunk | Upstream (`fileformats/`) | Rust (`ocio/src/fileformats/ctf/`) | Port tests |
|---|---|---|---|
| 4.5a | `ctf/CTFTransform.cpp:1-510` and `CTFTransform.h` (~600): `CTFVersion` (`ReadVersion`, comparisons), `CTFReaderTransform`, the metadata (`fromMetadata`, `toMetadata`, `GetElementsValues`); `ctf/IndexMapping.cpp/.h` (108); `ctf/CTFReaderUtils.cpp/.h` (284): the bit-depth, interpolation, style and attribute-name tables | `ctf_transform.rs`, `index_mapping.rs`, `ctf_reader_utils.rs` | `CTFTransform_tests.cpp` `read_version`, `accessors`; `IndexMapping_tests.cpp` (3) |
| 4.5b | `FileFormatCTF.cpp:1-700` (~600): `XMLParserHelper` (the line loop, the expat callbacks, the element factory by tag and version, the error texts "CTF/CLF parsing error ... At line (N)"), `LocalCachedFile`, the format infos | `file_format_ctf.rs` | `FileFormatCTF_tests.cpp`: the parser's structure tests (missing tags, wrong nesting, versions, empty files: ~30) |
| 4.5c | `ctf/CTFReaderHelper.cpp:1-800` (~650): `CTFReaderTransformElt`, `CTFReaderArrayElt`, `CTFArrayMgt`, `CTFReaderIndexMapElt` and `CTFIndexMapMgt` (the IndexMap pairs), `CTFReaderMetadataElt`, `CTFReaderInfoElt`, the descriptors | `ctf_reader_helper.rs` | ~25 metadata, info and array tests |
| 4.5d | `CTFReaderHelper.cpp:800-1200` (~350): `CTFReaderOpElt` (bit depths, ids, names, descriptions), the versioned factory (`ADD_READER_FOR_VERSIONS_UP_TO` and siblings); `CTFReaderMatrixElt`, `_1_3`, `CTFReaderRangeElt`, `_1_7`, `CTFReaderRangeValueElt` (`4561-5013`) | `ctf_reader_helper.rs` | ~25 matrix and range tests |
| 4.5e | `CTFReaderLogElt`, `_2_0`, `CTFReaderLogParamsElt`, `_2_0` (`3545-4084`); `CTFReaderGammaElt`, `_1_5`, `_CTF_2_0`, `_CLF_3_0`, `CTFReaderGammaParamsElt`, `_1_5` (`1862-2203`); `CTFReaderCDLElt`, `CTFReaderSatNodeElt`, `CTFReaderSOPNodeElt` (`1327-1416`) (~800) | `ctf_reader_helper.rs` | ~35 log, gamma (exponent, moncurve) and CDL tests |
| 4.5f | `CTFReaderLut1DElt`, `_1_4`, `_1_7`, `CTFReaderInvLut1DElt` (`3245-3432, 4085-4388`); `CTFReaderLut3DElt`, `_1_7`, `CTFReaderInvLut3DElt` (`3433-3544, 4389-4560`): half domain, raw halfs, IndexMap, hue adjust (~700) | `ctf_reader_helper.rs` | ~30 Lut1D and Lut3D tests; the file-based `Lut1DOpCPU_tests.cpp`, `Lut3DOp_tests.cpp`, `Lut1DOpData`/`Lut3DOpData` tests (WP 4.10) |
| 4.5g | `CTFReaderACESElt`, `CTFReaderACESParamsElt`, `CTFReaderFixedFunctionElt`, `CTFReaderFunctionElt` (`1199-1326, 1417-1556`); `CTFReaderGradingRGBCurveElt`, `CTFReaderGradingCurveElt`, `CurvePointsElt`, `CurveSlopesElt` (`2580-2973`) (~550) | `ctf_reader_helper.rs` | ~20 fixed-function and RGB curve tests |
| 4.5h | `CTFReaderReferenceElt` (`5014-5117`); `FileFormatCTF.cpp:700-1328`: `buildFileOps`, the references' resolution through 4.1c (~450) | `file_format_ctf.rs`, `ctf_reader_helper.rs` | `ReferenceOpData_tests.cpp` (12); `FileTransform_tests.cpp` CTF parts; `CDLTransform_tests.cpp` `faulty_file_content` (from 4.4e) |
| 4.5i (P5) | `CTFReaderExposureContrastElt`, `CTFReaderECParamsElt`, `CTFReaderDynamicParamElt`, `CTFReaderGradingPrimaryElt`, `ParamElt`, `CTFReaderGradingToneElt`, `ParamElt`, `CTFReaderGradingHueCurveElt` (`1557-1861, 2204-2579, 2659-2737, 2974-3244`) (~900) | `ctf_reader_helper.rs` | the ~13 grading, exposure-contrast and dynamic-parameter reader tests |

- `FileFormatCTF_tests.cpp` has 231 tests: 150 `FileFormatCTF` (reading) and 81 `CTFTransform`
  (writing and round trips). The table's counts are estimates; each chunk lists its tests in
  `upstream-map.toml`.
- Until 4.5i lands, the Phase 5 elements give "not ported yet" (P4-2).

## WP 4.6: the CLF/CTF writer (`ocio`)

| Chunk | Upstream (`fileformats/ctf/CTFTransform.cpp`) | Rust (`ocio/src/fileformats/ctf/`) | Port tests |
|---|---|---|---|
| 4.6a | `510-812` (~250): `WriteDescriptions`, `WriteValue` (`nan`, `inf`, `-inf`), `SetOStream` (width 11 and precision 8 for `float`, 19 and `DOUBLE_PRECISION` for `double`), `WriteValues` per bit depth, `OpWriter`, `GetValidatedFileBitDepth` | `ctf_writer.rs` | ~10 `CTFTransform` write tests of matrices |
| 4.6b | `MatrixWriter`, `RangeWriter`, `LogWriter`, `GammaWriter` (with `AddGammaParams`), `CDLWriter` (`813-936, 1123-1279, 1896-2041, 2284-2517`) (~550) | `ctf_writer.rs` | ~25 write tests |
| 4.6c | `Lut1DWriter`, `Lut3DWriter` (`2042-2283`): half domain, raw halfs, the per-bit-depth widths (~220) | `ctf_writer.rs` | ~15 write tests |
| 4.6d | `FixedFunctionWriter` (`1053-1122`), `GradingRGBCurveWriter` (`1502-1633`) (~180) | `ctf_writer.rs` | ~8 write tests |
| 4.6e | `TransformWriter` (`2518-2700`): the header, `compCLFversion="3"` for CLF, the lowest CTF version that fits (`GetMinimumVersion`), the id (`CacheIDHash` of the ops), info and metadata, the CLF check of its ops; `FileFormatCTF`'s `write` and `bake`; `GetNumWriteFormats`, `GetFormatNameByIndex` (~350; `GroupTransform::write` landed in 4.4e) | `ctf_writer.rs`, `file_format_ctf.rs`, `transforms/group_transform.rs` | `CTFTransform_tests.cpp` `version_write`; the round-trip tests; `GroupTransform_tests.cpp` `write_formats`, `write_with_noops`; oracle `write_transform` (O4.3) |
| 4.6f (P5) | `ExposureContrastWriter`, `GradingPrimaryWriter`, `GradingHueCurveWriter`, `GradingToneWriter` (`937-1052, 1280-1501, 1634-1895`) (~550) | `ctf_writer.rs` | the ~17 grading and exposure-contrast write tests |

## WP 4.7: the ICC reader (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.7a | `ext/sampleicc/src/include/iccProfileReader.h` (615) and the parts of `icProfileHeader.h` it uses: big-endian reads, the header and its magic number, the tag table (its count limit), `XYZ`, `curv`, `para` (types 0–4), `desc` and `dscm` (P4-4) | `ocio/src/fileformats/icc/` | `FileFormatICC_tests.cpp` `types`, `endian` |
| 4.7b | `fileformats/FileFormatICC.cpp/.h` (694): matrix/TRC profiles only, with their "Error parsing .icc file (...)" texts, the Bradford D50 to D65 adaptation, `curv` with 1 entry as a u8.8 gamma and with N entries as a 16-bit Lut1D, `para` type 0 as a Gamma and the other types as a 1,024-entry Lut1D, the same type on all three channels, the profile description (`GetProfileDescriptionFromICCProfile`) | `fileformats/file_format_icc.rs` | `FileFormatICC_tests.cpp` `test_file`, `test_apply`, `test_apply_para_t1` to `_t4`; `Config::instantiateDisplayFromICCProfile` without its monitor half (Phase 9) |

## WP 4.8: the baker (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.8a | `Baker.cpp` (335), `BakingUtils.cpp/.h` (~120): the settings, the shaper, input and target spaces, display and view, looks, the cube and shaper sizes, the validation texts, `GetShaperRange`, `GetInputToShaperProcessor` and siblings | `baker.rs`, `baking_utils.rs` | `Baker_tests.cpp` `baking_validation`; oracle `bake` (O4.4) |
| 4.8b | The `bake` of spi1d, spi3d, Iridas cube, Resolve cube, itx (4.2's files) | the formats' files | `Baker_tests.cpp` `bake_3dlut`; their formats' bake tests |
| 4.8c | The `bake` of flame and lustre (3DL), cinespace (CSP), Houdini, Truelight (4.3's files) | the formats' files | their bake tests |
| 4.8d | The `bake` of CLF and CTF (`FileFormatCTF.cpp`, through the writer of WP 4.6) | `file_format_ctf.rs` | their bake tests |

- Bakers sample processors through the CPU renderers: their text depends on the CPU and,
  where libm is called, on the platform. Their tests are in `cpu-tests`.
- Floats are written through iostreams (`cfmt`); each format's widths and precisions are
  checked against the wheel, not read off the source alone.

## WP 4.9: `.ocioz` archives (`ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 4.9a | `OCIOZArchive.cpp:390-663` (~260): reading an archive (the entries by path and by extension), `CIOPOciozArchive` (`getConfigData`, `getLutData`, `getFastLutFileHash`, `buildEntries`), `Config::CreateFromFile`'s archive branch (P4-3) | `ocioz_archive.rs`, `config.rs` | `OCIOZArchive_tests.cpp` `context_test_for_search_paths_and_filetransform_source_path`; `Config_tests.cpp` `create_from_archive` |
| 4.9b | `OCIOZArchive.cpp:149-389` (~200): `Config::archive` (`archiveConfig`: the config and the files of its search paths, deflate at the best level), `ExtractOCIOZArchive` | `ocioz_archive.rs`, `config.rs` | `OCIOZArchive_tests.cpp` `archive_config_and_compare_to_original`, `extract_config_and_compare_to_original`, `is_config_archivable`; oracle `ocioz` (O4.5) |

## WP 4.10: the tests of earlier phases that read files

| Chunk | What | Port tests |
|---|---|---|
| 4.10a | Ops and optimizer | `Lut1DOpCPU_tests.cpp` (8 file tests and the 3 `make_fast_from_inverse_*`), `Lut1DOp_tests.cpp`, `Lut1DOpData_tests.cpp`, `Lut3DOp_tests.cpp` (`cpu_renderer_cloned`, `cpu_renderer_inverse`, `cpu_renderer_lut3d_with_nan`), `Lut3DOpData_tests.cpp` (`compose`, `compose_2`, `inv_lut3d_lut_size`), `OpOptimizers_tests.cpp` (the 12 listed in `docs/cards/phase2.md`) |
| 4.10b | Configs and processors | `Config_tests.cpp` group F (9, `docs/cards/phase3.md`); `CPUProcessor_tests.cpp` `with_one_1d_lut`, `with_several_ops`, `image_desc`; `Caching_tests.cpp` `processor_cache`; `Processor_tests.cpp` file tests; `ColorSpace_tests.cpp`, `LookTransform_tests.cpp` and `DisplayViewTransform_tests.cpp` file tests; `GpuShader_tests.cpp` file tests (Metal, Vulkan) |

---

## Upstream tests

| File | Tests | Phase 4 chunks | Waits |
|---|---:|---|---|
| `fileformats/FileFormatCTF_tests.cpp` | 231 | ~201 (4.5b–h, 4.6a–e) | ~30 grading and exposure-contrast tests (P5: 4.5i, 4.6f) |
| `fileformats/FileFormatResolveCube_tests.cpp` | 11 | 11 (4.2d, 4.8b) | — |
| `fileformats/FileFormatCSP_tests.cpp` | 8 | 8 (4.3b, 4.8c) | — |
| `fileformats/FileFormatHDL_tests.cpp` | 8 | 8 (4.3c, 4.8c) | — |
| `fileformats/FileFormatICC_tests.cpp` | 8 | 8 (4.7a–b) | — |
| `fileformats/FileFormatSpi1D_tests.cpp` | 6 | 6 (4.2a, 4.8b) | — |
| `fileformats/FileFormatIridasCube_tests.cpp` | 6 | 6 (4.2c, 4.8b) | — |
| `fileformats/FileFormatDiscreet1DL_tests.cpp` | 6 | 6 (4.3d) | — |
| `fileformats/FileFormatCC_tests.cpp` | 6 | 6 (4.4d–e) | — |
| `fileformats/xmlutils/XMLReaderUtils_tests.cpp` | 6 | 6 (4.4c) | — |
| `fileformats/FileFormatSpi3D_tests.cpp` | 5 | 5 (4.2b, 4.8b) | — |
| `fileformats/FileFormat3DL_tests.cpp` | 5 | 5 (4.3a, 4.8c) | — |
| `fileformats/FileFormatSpiMtx_tests.cpp` | 4 | 4 (4.2a) | — |
| `fileformats/FileFormatIridasLook_tests.cpp` | 4 | 4 (4.4f) | — |
| `fileformats/FileFormatTruelight_tests.cpp`, `Pandora`, `VF` | 9 | 9 (4.3e, 4.8c) | — |
| `fileformats/FileFormatCDL_tests.cpp`, `CCC` | 4 | 4 (4.4e) | — |
| `fileformats/FileFormatIridasItx_tests.cpp` | 2 | 2 (4.2e, 4.8b) | — |
| `fileformats/ctf/CTFTransform_tests.cpp`, `IndexMapping_tests.cpp` | 6 | 6 (4.5a, 4.6e) | — |
| `transforms/FileTransform_tests.cpp` | 10 | 8 (4.1a–c; `basic`, `validate` landed in 3.1c) | — |
| `transforms/CDLTransform_tests.cpp` | 11 | 5 (4.4e), `faulty_file_content` (4.5h) | — |
| `transforms/GroupTransform_tests.cpp` | 3 | `write_formats`, `write_with_noops` (4.6e) | `basic` (P2, `p3-after-p2`) |
| `ops/reference/ReferenceOpData_tests.cpp` | 12 | 11 (4.5h; `accessors` may land earlier) | — |
| `ParseUtils_tests.cpp` | 11 | 5 (4.0a) | — |
| `Baker_tests.cpp` | 2 | 2 (4.8a–b) | — |
| `OCIOZArchive_tests.cpp` | 4 | 4 (4.9a–b) | — |
| `Caching_tests.cpp` | 2 | 2 (4.1b, 4.10b) | — |
| Phase 2 and 3 file tests | ~60 | WP 4.10 | those that also need Phase 5 (dynamic properties) |

About 400 upstream tests land in Phase 4, and ~30 more with Phase 5's CTF arms. The GPU suite
(`tests/gpu`, which reads LUT files too) is Phase 7's; the Python suite's file tests are
Phase 6's.

## Oracle support (owner-reviewed chunks)

Each command lives in its own module under `oracle/ocio_oracle/` and reports what the library
does. Strings and file contents come back as bytes (hex or blobs). Each call captures the log.

| Chunk | Command (module) | What |
|---|---|---|
| O4.1 | `file_formats` (`file_formats_api.py`) | The registries in order: `FileTransform`'s formats (name, extension, capabilities), `Baker`'s formats, `GroupTransform`'s write formats, and `IsFormatExtensionSupported` of given extensions |
| O4.2 | `with_files` (`files_api.py`) | Runs another oracle command (`processor_ops`, `cpu_apply`, `image_apply`, `gpu_shader`, `config_calls`, `transform_text`) in a new temporary directory holding given files (text, or bytes from blobs), with `$FILES` in the command's string arguments replaced by the directory's absolute path, and back in its results. Reading files then needs no change to those modules. It reports `ExceptionMissingFile` by its type |
| O4.3 | `write_transform` (`write_api.py`) | `GroupTransform.write(formatName, config)` of a transform spec, or of a processor's `createGroupTransform()`: the bytes, or the exception |
| O4.4 | `bake` (`bake_api.py`) | A `Baker` on a config (YAML, with files through O4.2's mechanism): every setter in order, then `bake()`: the bytes, the exception and the log |
| O4.5 | `ocioz` (`ocioz_api.py`) | `Config.archive` of a config with files: the archive's entries in order (name, method, level flags, uncompressed contents as blobs), not its compressed bytes (P4-3); `ExtractOCIOZArchive` of a given archive: the files it writes |
| O4.6 | Fixture group `test_files` (`regen.py`) | For each file of `tests/data/files` OCIO reads: `processor_ops` of a `FileTransform` of it (both directions, `OPTIMIZATION_NONE`), or the exception, and the log; where `oracle check-all` shows them the same on both platforms and the ops depend on no CPU kernel |
| T4.1 | `ocio-testkit` `crt.rs` (not the oracle) | The C runtime's `sscanf` (glibc) and `sscanf_s` (UCRT) for the readers' formats, and MSVC's text-mode `ifstream`: the platform references of 4.0b and 4.0c. Reviewed like an oracle chunk |

- O4.1–O4.5 and T4.1 land first, in card `p4-oracle`. O4.6 lands with `p4-m2-parity`.
- Pixels and GPU text of LUT files are never committed: O4.2 runs them live.

## Owner decisions

**Decided by the owner on 2026-10-07, all nine as recommended.** P4-3's deflate bytes are
deviation D-6, added to `docs/deviations.md` with chunk 4.9b.

| # | Decision | Recommendation (decided) |
|---|---|---|
| P4-1 | **The XML parser** (`dependency`). PLAN.md §9 picks `quick-xml` ≥ 0.41 | **Port the parts of expat that OCIO reaches** (MIT; `upstream/expat` pinned at the wheels' version, with expat's own tests copied), as D1 did for yaml-cpp. OCIO's messages carry expat's error texts ("no element found", "mismatched tag", "not well-formed (invalid token)") and the line it stops at, and its character-data splitting decides what text an element sees. `quick-xml` would read well-formed files the same, but its errors, encodings and DTD handling differ, and a compatibility layer could not reproduce which line expat stops at. ~4,300 lines (4.4a–b). Chunk 4.4a first pins the version each wheel bundles: the Windows DLL carries expat 2.7-era strings (`EXPAT_MALLOC_DEBUG`); CMake asks for ≥ 2.6.0 and recommends 2.7.2. If the two wheels bundle different versions, the port follows each (D12) |
| P4-2 | **CTF elements of Phase 5's ops** (ExposureContrast, GradingPrimary, GradingTone, GradingHueCurve) | **Phase 4 refuses them with "not ported yet"; Phase 5 adds their reader and writer arms (4.5i, 4.6f) with its op data.** ~30 CTF tests wait. GradingRGBCurve's arms are Phase 4's, since its op data is Phase 2's. The alternative: port the four op data now, without renderers |
| P4-3 | **`.ocioz` archives** (`dependency`). PLAN.md §9 picks `zip` 8.x | **Read with `zip` (pinned, deflate only), and compare archives by their entries, not their bytes.** Upstream's archives aren't reproducible: `Config::archive` stamps each entry with `time(NULL)` (`OCIOZArchive.cpp:272`) and lists directories in the file system's order, and zlib 1.3.1's deflate output (the Windows wheel's) would need its compressor ported. So the port writes the same entries, names, order of the config and its search paths, method and level, and its own deflate stream; reading archives is exact. The deflate bytes are deviation D-6 |
| P4-4 | **ICC** (`dependency`: a third-party source) | **Translate the subset of SampleICC 1.2.6 that `iccProfileReader.h` uses**, under the ICC Software License 0.2 (BSD-like, attribution) with its notice in `NOTICE`, as for pystring. ~600 lines. The alternative is to write the reader from the ICC specification and check it against the wheel |
| P4-5 | **Public API** (`api`) | (a) `FileTransform::formats()` returns the registry's (name, extension, capabilities) in order, besides upstream's index getters. (b) `Baker` mirrors upstream (`new`, setters taking `impl AsRef<[u8]>`, `bake(&mut impl Write) -> Result<()>`). (c) `GroupTransform::write(&self, config, format, &mut impl Write)`; `Config::archive(&self, &mut impl Write)`; `extract_ocioz_archive(archive, destination)`. (d) `CdlTransform::from_file(src, cccid) -> Result<CdlTransform>` and `group_from_file(src)`. (e) `ConfigIoProxy::lut_data(path) -> Result<Vec<u8>>`, as 3.10c's trait grows. (f) `clear_all_caches()` also empties the file caches. Paths stay bytes (D4) |
| P4-6 | **Windows text mode** | **Match it.** Readers open text formats with `std::ios_base::in` (`FileTransform.cpp:634, 703`): on Windows, `CR LF` reads as `LF`; on Linux, `CR` reaches the reader. A CRLF file can read differently on the two platforms (and through a `ConfigIOProxy`, which never translates). The port reproduces each (D12), with an `I-` entry. 4.0c pins `0x1A` and lone `CR` against MSVC's runtime |
| P4-7 | **Hostile files** | **Errors where upstream would exhaust memory or crash** (the owner's general rule), each a `U-` entry: a 3D LUT size that overflows `int` or allocates terabytes, recursive CTF references beyond the stack, an ICC tag offset past the end. Fuzz each reader under `run-capped` (8 GB, 20 min) with generated files in the card's test target; no new dependency (`cargo-fuzz` needs nightly) |
| P4-8 | **LUT files beyond upstream's** (`dependency`) | **None.** Upstream's 258 files plus generated files cover every reader. A vendor LUT corpus would need licences for little more coverage. Revisit for M2 if a reader's generator misses a case |
| P4-9 | **Oracle chunks O4.1–O4.6 and T4.1** (`oracle`) | Reviewed by a verifier agent each, as since 2026-10-05 |

## Platform risks

- **Number parsing.** `NumberUtils::from_chars` (both wheels' branches, Phase 0) for floats;
  `istream >> int` (`num_get`, 3.3f) for `StringToInt`; `sscanf` (glibc) and `sscanf_s`
  (UCRT) in spi1d, Iridas cube and Discreet 1DL, whose `%d` overflow and `%63s` truncation are
  the C runtimes'. Each reader sweeps its numbers against both wheels.
- **Text mode** (P4-6): CRLF and `0x1A` on Windows; `CR` kept on Linux.
- **Writers' floats.** The CTF writer's `float` width 11 and precision 8, `double` width 19
  (`SetOStream`), and the bakers' iostream settings go through `cfmt`; NaN and infinities are
  written as `nan`, `inf` and `-inf` by the CTF writer and as each platform's iostreams spell
  them elsewhere. The memory notes say the CTF widths and the LUT writers were never checked
  against the wheel: 4.6a and 4.8b do it first.
- **CPU-dependent bytes.** Baked LUTs, fast inverses and composed LUTs in readers depend on
  the CPU's kernels: `cpu-tests`, SDE, live only.
- **expat.** The wheels may bundle different expat versions (P4-1); its error texts and
  amplification limits change between versions.
- **Paths.** UTF-8 to UTF-16 on Windows (lossy for invalid bytes), `nt` and `posix` `os.path`
  (Phase 3), the file cache keyed by absolute path and file hash (`st_dev:st_ino` on Linux,
  `st_dev:std::hash(path)` on Windows: machine-specific, checked live).
- **Locale.** Classic "C" everywhere (D-1); readers imbue it explicitly where upstream does.
- **Archives.** Wall-clock timestamps and directory order make upstream's archives
  irreproducible (P4-3).

## Order and parallelism

```
p4-oracle (O4.1-O4.5, T4.1) ─────────── oracle checks of every card below
p4-parse (4.0a-c) ─┬─ p4-registry (4.1a-c) ─┬─ p4-lut-text-1 (4.2a-e) ─┬─ p4-baker-1 (4.8a-b) ─┐
                   │                        ├─ p4-lut-text-2 (4.3a-e) ─┴─ p4-baker-2 (4.8c)    │
                   │                        └─ p4-icc (4.7a-b)                                  │
                   └─ p4-xml (4.4a-c) ─┬─ p4-cdl (4.4d-f)                                       │
                                       └─ p4-ctf-read-1 (4.5a-d) ─ p4-ctf-read-2 (4.5e-h) ─┐    │
                                                       p4-ctf-write (4.6a-e) ─ p4-baker-3 (4.8d)
p3-loading ─ p4-ocioz (4.9a-b)                                                         │
all of the above ── p4-file-tests (4.10a-b) ── p4-m2-parity (O4.6) ── M2               │
Phase 5 ── p4-after-p5 (4.5i, 4.6f) ───────────────────────────────────────────────────┘
```

- **Implementer A (XML, the critical path):** `p4-xml` → `p4-cdl` → `p4-ctf-read-1` →
  `p4-ctf-read-2` → `p4-ctf-write`.
- **Implementer B (line formats and bakers):** `p4-parse` → `p4-lut-text-1` →
  `p4-lut-text-2` → `p4-baker-1` → `p4-baker-2` → `p4-baker-3`.
- **Implementer C (oracle, registry, binary formats):** `p4-oracle` → `p4-registry` →
  `p4-icc` → `p4-ocioz` → `p4-file-tests` → `p4-m2-parity`.
- **Verifier:** reviews every card before it lands, with mutation testing.
- With two implementers, C's cards go to whoever is free, `p4-oracle` and `p4-registry` first.

## Cards

Each card is one branch and one PR. "Now" means the card can start once its owner decisions
are made, without waiting for other Phase 4 cards.

| Card | Chunks | Size | Who | Needs | Owner items |
|---|---|---|---|---|---|
| `p4-oracle` | O4.1–O4.5, T4.1, each its own chunk | 6 | C | now | `oracle` (P4-9) |
| `p4-parse` | 4.0a–4.0c | 3, ~600 | B | now (3.3f's `num_get` has landed); T4.1 | — |
| `p4-registry` | 4.1a–4.1c | 3, ~700 | C | `p4-parse`; 3.10c (`ConfigIOProxy`) for 4.1b's proxy branch | `api` (P4-5 a, f) |
| `p4-lut-text-1` | 4.2a–4.2e | 5, ~2,100 | B | `p4-registry` | — |
| `p4-lut-text-2` | 4.3a–4.3e | 5, ~2,950 | B | `p4-registry` | — |
| `p4-xml` | 4.4a–4.4c | 3, ~5,300 | A | `p4-parse` | — (P4-1 decided) |
| `p4-cdl` | 4.4d–4.4f | 3, ~1,850 | A | `p4-xml`, `p4-registry` | `api` (P4-5 d) |
| `p4-ctf-read-1` | 4.5a–4.5d | 4, ~2,200 | A | `p4-xml`, `p4-registry` | — |
| `p4-ctf-read-2` | 4.5e–4.5h | 4, ~2,500 | A | `p4-ctf-read-1` | — |
| `p4-ctf-write` | 4.6a–4.6e | 5, ~1,550 | A | `p4-ctf-read-2` (round trips) | `api` (P4-5 c) |
| `p4-icc` | 4.7a–4.7b | 2, ~1,300 | C | `p4-registry` | — (P4-4 decided; the `NOTICE` entry lands with 4.7a) |
| `p4-baker-1` | 4.8a–4.8b | 2, ~900 | B | `p4-lut-text-1`; `p3-loading` (configs) | `api` (P4-5 b) |
| `p4-baker-2` | 4.8c | 1, ~500 | B | `p4-baker-1`, `p4-lut-text-2` | — |
| `p4-baker-3` | 4.8d | 1, ~150 | B | `p4-baker-1`, `p4-ctf-write` | — |
| `p4-ocioz` | 4.9a–4.9b | 2, ~500 | C | `p4-registry`, `p3-loading` | — (P4-3 decided: the `zip` pin and D-6 land with 4.9a-b) |
| `p4-file-tests` | 4.10a–4.10b | 2 | C | every reader card, `p3-loading`, `p3-builders` | — |
| `p4-m2-parity` | O4.6, then every file of `tests/data/files` and each reader's generator through the port's API against the wheel: ops, CPU at every bit depth, layout and optimization level, GPU in all 10 languages; written and baked text | 2–3 | any | every card above | `oracle` (O4.6) |
| `p4-after-p5` | 4.5i, 4.6f | 2, ~1,450 | A | Phase 5's op data, `p4-ctf-write` | — |

18 cards, 54 chunks. When a card grows past about 6 chunks, it splits at a dependency boundary.

**Can start now:** `p4-oracle`, `p4-parse`, then `p4-registry` and
`p4-xml`. They need nothing beyond what Phase 3 has landed, except `p4-registry`'s
`ConfigIOProxy` branch (3.10c, in `p3-loading`).

**Waits for other phases:**
- **Phase 3:** the config tests (`p3-loading`), `ConfigIOProxy` (3.10c), the bakers' configs.
- **Phase 5:** the CTF arms of its ops (P4-2), the dynamic-property tests that read files.
- **Phase 6:** the Python suite's file tests. **Phase 7:** the GPU suite. **Phase 9:**
  `instantiateDisplayFromMonitorName` (SystemMonitor). **Phase 10:** `ociobakelut`,
  `ociowrite`, `ociomakeclf`, `ocioarchive`, `ociochecklut`.

## Phase 4 exit (M2)

Through OCIO's API, on Windows and Rocky Linux 9:
- every file of `tests/data/files` and every generated file reads to the same ops, pixels (CPU
  at every bit depth, layout and optimization level; GPU in all 10 languages) and messages as
  the wheel, or is refused with the same message;
- CLF, CTF, CDL, CC and CCC written by `GroupTransform::write`, and the 12 baked formats, are
  byte-exact (per platform and CPU where those make them differ);
- configs read from and written to `.ocioz` archives match the wheel's, entry by entry (P4-3);
- the upstream tests of the slice are ported and counted in `docs/parity.md`, except those
  waiting for Phases 5, 6, 7, 9 and 10, each listed in `upstream-map.toml` with its reason.
