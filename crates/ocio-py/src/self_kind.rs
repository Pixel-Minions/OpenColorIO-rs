// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! How pybind11 loads the `self` of each method overload bound without argument names, and
//! the port's error where the wheel's process dies (U-134, owner decision 2026-10-22).
//!
//! An overload bound with `py::arg` names (or `py::pos_only`) gets an argument record for
//! `self` that refuses `None` (`none = false`, pybind11's `process_attribute<arg>`). Without
//! names there is no record, and pybind11's class caster loads `None` as a null pointer when
//! it may convert (`type_caster_generic::load_impl`): for a function with one overload, or in
//! the second pass of an overloaded one (which only overloads with a positional parameter
//! besides `self` reach). What happens then depends on the C++ parameter `self` binds to:
//! - [`SelfKind::Pointer`]: a member function pointer (`&Class::method`), called through the
//!   null `this`, or a lambda taking `Class *`;
//! - [`SelfKind::Holder`]: a lambda taking the class's `shared_ptr` (`ClassRcPtr & self`,
//!   `defRepr`'s `OCIO_SHARED_PTR<T> &`), which gets an empty one and dereferences it;
//! - [`SelfKind::Reference`]: a lambda taking `Class &` (the iterators, `bind_vector`,
//!   `defRepr` of a class held by value), whose cast throws `reference_cast_error`: pybind11
//!   tries the next overload, and raises its "incompatible function arguments" TypeError;
//! - [`SelfKind::Iterator`]: a lambda taking the class's `shared_ptr` that only builds an
//!   iterator holding it (`ConfigRcPtr & self` → `RoleNameIterator(self)`,
//!   `GroupTransform.__iter__`): the iterator, holding the empty one, is returned and lives;
//!   its `__iter__` and its `repr` work, its `__len__`, `__getitem__` and `__next__` read
//!   through the null pointer.
//!
//! The first two use a null pointer: the wheel's process dies (probed on Windows,
//! 2026-10-22, in a process per probe: `crates/ocio-py/tests/none_self_oracle.rs`). The port
//! raises ValueError instead ([`null_self`]); it raises pybind11's TypeError for the third,
//! as the wheel does. For the fourth it returns the iterator, whose methods that read the
//! null pointer (where the wheel's process dies) raise ValueError ([`null_iterator`]).
//!
//! [`UNNAMED`] lists every such overload of the wheel's classes, by its class path, name and
//! index among the function's overloads, with its kind, read from upstream's bindings
//! (each group cites its file, each entry its line) and checked against the wheel's
//! signatures (`none_self_oracle.rs`: the table is the wheel's unnamed overloads, exactly).
//! Constructors aren't listed: pybind11 checks their `self` first (`dispatch.rs`).

/// How an unnamed overload's `self` binds (see the module's documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    Pointer,
    Holder,
    Reference,
    Iterator,
}

use SelfKind::{Holder, Iterator, Pointer, Reference};

/// The kind of the unnamed overload `index` of the method `name` of the class `class` (its
/// path in the module, `Config.ColorSpaceIterator`), if there is one.
pub fn kind(class: &str, name: &str, index: usize) -> Option<SelfKind> {
    UNNAMED
        .iter()
        .find(|(c, n, i, _)| *c == class && *n == name && usize::from(*i) == index)
        .map(|e| e.3)
}

/// The port's error for `None` as the `self` of `class.name`, where the wheel calls through a
/// null pointer (U-134).
pub fn null_self(class: &str, name: &str) -> pyo3::PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{class}.{name}: None as self, a null pointer the wheel calls the method through (its \
         process dies; docs/improvements.md, U-134)"
    ))
}

/// The port's error for a method of the iterator that `class.name` made of `None` as `self`
/// ([`SelfKind::Iterator`]), where the wheel reads through the iterator's null pointer
/// (U-134).
pub fn null_iterator(class: &str, name: &str) -> pyo3::PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "the iterator {class}.{name} made of None as self, a null pointer the wheel's iterator \
         reads through (its process dies; docs/improvements.md, U-134)"
    ))
}

/// Every method overload of the wheel's classes bound without argument names: its class path,
/// name, index among the function's overloads, and how its `self` binds.
#[rustfmt::skip]
pub static UNNAMED: &[(&str, &str, u8, SelfKind)] = &[
    // src/bindings/python/PyBaker.cpp @ v2.5.2
    ("Baker", "getConfig", 0, Pointer),  // :86
    ("Baker", "getFormat", 0, Pointer),  // :90
    ("Baker", "getFormatMetadata", 0, Pointer),  // :94
    ("Baker", "getInputSpace", 0, Pointer),  // :98
    ("Baker", "getShaperSpace", 0, Pointer),  // :102
    ("Baker", "getLooks", 0, Pointer),  // :106
    ("Baker", "getDisplay", 0, Pointer),  // :110
    ("Baker", "getView", 0, Pointer),  // :112
    ("Baker", "getTargetSpace", 0, Pointer),  // :116
    ("Baker", "getShaperSize", 0, Pointer),  // :120
    ("Baker", "getCubeSize", 0, Pointer),  // :124
    ("Baker", "bake", 1, Holder),  // :135
    ("Baker.FormatIterator", "__len__", 0, Reference),  // :144
    ("Baker.FormatIterator", "__getitem__", 0, Reference),  // :145
    ("Baker.FormatIterator", "__iter__", 0, Reference),  // :151
    ("Baker.FormatIterator", "__next__", 0, Reference),  // :155
    // src/bindings/python/PyBuiltinConfigRegistry.cpp @ v2.5.2
    ("BuiltinConfigRegistry", "__iter__", 0, Reference),  // :33
    ("BuiltinConfigRegistry", "__len__", 0, Reference),  // :38
    ("BuiltinConfigRegistry", "__getitem__", 0, Reference),  // :43
    ("BuiltinConfigRegistry", "__contains__", 0, Reference),  // :48
    ("BuiltinConfigRegistry", "getBuiltinConfigs", 0, Reference),  // :61
    ("BuiltinConfigRegistry.BuiltinConfigIterator", "__len__", 0, Reference),  // :67
    ("BuiltinConfigRegistry.BuiltinConfigIterator", "__getitem__", 0, Reference),  // :68
    ("BuiltinConfigRegistry.BuiltinConfigIterator", "__iter__", 0, Reference),  // :77
    ("BuiltinConfigRegistry.BuiltinConfigIterator", "__next__", 0, Reference),  // :78
    ("BuiltinConfigRegistry.BuiltinConfigNameIterator", "__len__", 0, Reference),  // :90
    ("BuiltinConfigRegistry.BuiltinConfigNameIterator", "__getitem__", 0, Reference),  // :91
    ("BuiltinConfigRegistry.BuiltinConfigNameIterator", "__iter__", 0, Reference),  // :95
    ("BuiltinConfigRegistry.BuiltinConfigNameIterator", "__next__", 0, Reference),  // :96
    // src/bindings/python/PyBuiltinTransformRegistry.cpp @ v2.5.2
    ("BuiltinTransformRegistry", "__iter__", 0, Reference),  // :41
    ("BuiltinTransformRegistry", "__len__", 0, Reference),  // :46
    ("BuiltinTransformRegistry", "__getitem__", 0, Reference),  // :48
    ("BuiltinTransformRegistry", "__contains__", 0, Reference),  // :64
    ("BuiltinTransformRegistry", "getBuiltins", 0, Reference),  // :77
    ("BuiltinTransformRegistry.BuiltinStyleIterator", "__len__", 0, Reference),  // :83
    ("BuiltinTransformRegistry.BuiltinStyleIterator", "__getitem__", 0, Reference),  // :84
    ("BuiltinTransformRegistry.BuiltinStyleIterator", "__iter__", 0, Reference),  // :89
    ("BuiltinTransformRegistry.BuiltinStyleIterator", "__next__", 0, Reference),  // :90
    ("BuiltinTransformRegistry.BuiltinIterator", "__len__", 0, Reference),  // :97
    ("BuiltinTransformRegistry.BuiltinIterator", "__getitem__", 0, Reference),  // :98
    ("BuiltinTransformRegistry.BuiltinIterator", "__iter__", 0, Reference),  // :104
    ("BuiltinTransformRegistry.BuiltinIterator", "__next__", 0, Reference),  // :105
    // src/bindings/python/PyCPUProcessor.cpp @ v2.5.2
    ("CPUProcessor", "isNoOp", 0, Pointer),  // :21
    ("CPUProcessor", "isIdentity", 0, Pointer),  // :23
    ("CPUProcessor", "hasChannelCrosstalk", 0, Pointer),  // :25
    ("CPUProcessor", "getCacheID", 0, Pointer),  // :27
    ("CPUProcessor", "getInputBitDepth", 0, Pointer),  // :29
    ("CPUProcessor", "getOutputBitDepth", 0, Pointer),  // :31
    ("CPUProcessor", "isDynamic", 0, Pointer),  // :44
    // src/bindings/python/PyColorSpace.cpp @ v2.5.2
    ("ColorSpace", "getName", 0, Pointer),  // :165
    ("ColorSpace", "getAliases", 0, Iterator),  // :177
    ("ColorSpace", "clearAliases", 0, Pointer),  // :181
    ("ColorSpace", "getFamily", 0, Pointer),  // :184
    ("ColorSpace", "getEncoding", 0, Pointer),  // :188
    ("ColorSpace", "getEqualityGroup", 0, Pointer),  // :192
    ("ColorSpace", "getDescription", 0, Pointer),  // :196
    ("ColorSpace", "getInteropID", 0, Pointer),  // :200
    ("ColorSpace", "getInterchangeAttributes", 0, Pointer),  // :208
    ("ColorSpace", "getBitDepth", 0, Pointer),  // :210
    ("ColorSpace", "getCategories", 0, Iterator),  // :222
    ("ColorSpace", "clearCategories", 0, Pointer),  // :226
    ("ColorSpace", "isData", 0, Pointer),  // :230
    ("ColorSpace", "getReferenceSpaceType", 0, Pointer),  // :234
    ("ColorSpace", "getAllocation", 0, Pointer),  // :238
    ("ColorSpace", "getAllocationVars", 0, Holder),  // :242
    ("ColorSpace", "__repr__", 0, Holder),  // :263
    ("ColorSpace.ColorSpaceCategoryIterator", "__len__", 0, Reference),  // :266
    ("ColorSpace.ColorSpaceCategoryIterator", "__getitem__", 0, Reference),  // :270
    ("ColorSpace.ColorSpaceCategoryIterator", "__iter__", 0, Reference),  // :275
    ("ColorSpace.ColorSpaceCategoryIterator", "__next__", 0, Reference),  // :279
    ("ColorSpace.ColorSpaceAliasIterator", "__len__", 0, Reference),  // :286
    ("ColorSpace.ColorSpaceAliasIterator", "__getitem__", 0, Reference),  // :290
    ("ColorSpace.ColorSpaceAliasIterator", "__iter__", 0, Reference),  // :295
    ("ColorSpace.ColorSpaceAliasIterator", "__next__", 0, Reference),  // :299
    // src/bindings/python/PyColorSpaceSet.cpp @ v2.5.2
    ("ColorSpaceSet", "__eq__", 0, Pointer),  // :49
    ("ColorSpaceSet", "__ne__", 0, Pointer),  // :51
    ("ColorSpaceSet", "__sub__", 0, Holder),  // :54
    ("ColorSpaceSet", "__or__", 0, Holder),  // :60
    ("ColorSpaceSet", "__and__", 0, Holder),  // :66
    ("ColorSpaceSet", "getColorSpaceNames", 0, Iterator),  // :73
    ("ColorSpaceSet", "getColorSpaces", 0, Iterator),  // :77
    ("ColorSpaceSet", "clearColorSpaces", 0, Pointer),  // :93
    ("ColorSpaceSet.ColorSpaceNameIterator", "__len__", 0, Reference),  // :97
    ("ColorSpaceSet.ColorSpaceNameIterator", "__getitem__", 0, Reference),  // :101
    ("ColorSpaceSet.ColorSpaceNameIterator", "__iter__", 0, Reference),  // :106
    ("ColorSpaceSet.ColorSpaceNameIterator", "__next__", 0, Reference),  // :110
    ("ColorSpaceSet.ColorSpaceIterator", "__len__", 0, Reference),  // :117
    ("ColorSpaceSet.ColorSpaceIterator", "__getitem__", 0, Reference),  // :118
    ("ColorSpaceSet.ColorSpaceIterator", "__iter__", 0, Reference),  // :123
    ("ColorSpaceSet.ColorSpaceIterator", "__next__", 0, Reference),  // :124
    // src/bindings/python/PyConfig.cpp @ v2.5.2
    ("Config", "getMajorVersion", 0, Pointer),  // :217
    ("Config", "getMinorVersion", 0, Pointer),  // :221
    ("Config", "upgradeToLatestVersion", 0, Pointer),  // :227
    ("Config", "validate", 0, Pointer),  // :229
    ("Config", "getName", 0, Pointer),  // :231
    ("Config", "getFamilySeparator", 0, Pointer),  // :235
    ("Config", "getDescription", 0, Pointer),  // :239
    ("Config", "serialize", 1, Holder),  // :250
    ("Config", "getCacheID", 0, Pointer),  // :257
    ("Config", "getCurrentContext", 0, Pointer),  // :265
    ("Config", "getEnvironmentVarNames", 0, Iterator),  // :269
    ("Config", "clearEnvironmentVars", 0, Pointer),  // :275
    ("Config", "getEnvironmentMode", 0, Pointer),  // :279
    ("Config", "loadEnvironment", 0, Pointer),  // :281
    ("Config", "getSearchPath", 0, Pointer),  // :283
    ("Config", "getSearchPaths", 0, Iterator),  // :287
    ("Config", "clearSearchPaths", 0, Pointer),  // :291
    ("Config", "getWorkingDir", 0, Pointer),  // :295
    ("Config", "getConfigIOProxy", 0, Pointer),  // :299
    ("Config", "getColorSpaceNames", 1, Holder),  // :323
    ("Config", "getColorSpaces", 2, Holder),  // :327
    ("Config", "clearColorSpaces", 0, Pointer),  // :341
    ("Config", "isStrictParsingEnabled", 0, Pointer),  // :345
    ("Config", "getInactiveColorSpaces", 0, Pointer),  // :351
    ("Config", "getRoleNames", 0, Iterator),  // :391
    ("Config", "getRoles", 0, Iterator),  // :395
    ("Config", "clearSharedViews", 0, Pointer),  // :419
    ("Config", "getSharedViews", 0, Iterator),  // :421
    ("Config", "getDefaultDisplay", 0, Pointer),  // :425
    ("Config", "getDisplays", 0, Iterator),  // :427
    ("Config", "getDisplaysAll", 0, Iterator),  // :431
    ("Config", "clearDisplays", 0, Pointer),  // :501
    ("Config", "clearVirtualDisplay", 0, Pointer),  // :546
    ("Config", "getActiveDisplays", 0, Iterator),  // :591
    ("Config", "clearActiveDisplays", 0, Pointer),  // :599
    ("Config", "getNumActiveDisplays", 0, Pointer),  // :601
    ("Config", "getActiveViews", 0, Iterator),  // :606
    ("Config", "clearActiveViews", 0, Pointer),  // :614
    ("Config", "getNumActiveViews", 0, Pointer),  // :616
    ("Config", "getDefaultLumaCoefs", 0, Holder),  // :620
    ("Config", "getLookNames", 0, Iterator),  // :637
    ("Config", "getLooks", 0, Iterator),  // :641
    ("Config", "clearLooks", 0, Pointer),  // :647
    ("Config", "getViewTransformNames", 0, Iterator),  // :653
    ("Config", "getViewTransforms", 0, Iterator),  // :657
    ("Config", "getDefaultSceneToDisplayViewTransform", 0, Pointer),  // :663
    ("Config", "getDefaultViewTransformName", 0, Pointer),  // :666
    ("Config", "clearViewTransforms", 0, Pointer),  // :672
    ("Config", "getNamedTransformNames", 1, Holder),  // :689
    ("Config", "getNamedTransforms", 1, Holder),  // :693
    ("Config", "clearNamedTransforms", 0, Pointer),  // :698
    ("Config", "getViewingRules", 0, Pointer),  // :701
    ("Config", "getFileRules", 0, Pointer),  // :707
    ("Config", "clearProcessorCache", 0, Pointer),  // :966
    ("Config", "isArchivable", 0, Pointer),  // :970
    ("Config", "archive", 0, Holder),  // :971
    ("Config", "__str__", 0, Holder),  // :980
    ("Config", "__repr__", 0, Holder),  // :986
    ("Config.EnvironmentVarNameIterator", "__len__", 0, Reference),  // :1014
    ("Config.EnvironmentVarNameIterator", "__getitem__", 0, Reference),  // :1018
    ("Config.EnvironmentVarNameIterator", "__iter__", 0, Reference),  // :1023
    ("Config.EnvironmentVarNameIterator", "__next__", 0, Reference),  // :1027
    ("Config.SearchPathIterator", "__len__", 0, Reference),  // :1034
    ("Config.SearchPathIterator", "__getitem__", 0, Reference),  // :1038
    ("Config.SearchPathIterator", "__iter__", 0, Reference),  // :1043
    ("Config.SearchPathIterator", "__next__", 0, Reference),  // :1044
    ("Config.ColorSpaceNameIterator", "__len__", 0, Reference),  // :1051
    ("Config.ColorSpaceNameIterator", "__getitem__", 0, Reference),  // :1055
    ("Config.ColorSpaceNameIterator", "__iter__", 0, Reference),  // :1063
    ("Config.ColorSpaceNameIterator", "__next__", 0, Reference),  // :1067
    ("Config.ColorSpaceIterator", "__len__", 0, Reference),  // :1077
    ("Config.ColorSpaceIterator", "__getitem__", 0, Reference),  // :1081
    ("Config.ColorSpaceIterator", "__iter__", 0, Reference),  // :1090
    ("Config.ColorSpaceIterator", "__next__", 0, Reference),  // :1091
    ("Config.ActiveColorSpaceNameIterator", "__len__", 0, Reference),  // :1102
    ("Config.ActiveColorSpaceNameIterator", "__getitem__", 0, Reference),  // :1106
    ("Config.ActiveColorSpaceNameIterator", "__iter__", 0, Reference),  // :1111
    ("Config.ActiveColorSpaceNameIterator", "__next__", 0, Reference),  // :1115
    ("Config.ActiveColorSpaceIterator", "__len__", 0, Reference),  // :1122
    ("Config.ActiveColorSpaceIterator", "__getitem__", 0, Reference),  // :1126
    ("Config.ActiveColorSpaceIterator", "__iter__", 0, Reference),  // :1132
    ("Config.ActiveColorSpaceIterator", "__next__", 0, Reference),  // :1136
    ("Config.RoleNameIterator", "__len__", 0, Reference),  // :1144
    ("Config.RoleNameIterator", "__getitem__", 0, Reference),  // :1145
    ("Config.RoleNameIterator", "__iter__", 0, Reference),  // :1150
    ("Config.RoleNameIterator", "__next__", 0, Reference),  // :1151
    ("Config.RoleColorSpaceIterator", "__len__", 0, Reference),  // :1158
    ("Config.RoleColorSpaceIterator", "__getitem__", 0, Reference),  // :1159
    ("Config.RoleColorSpaceIterator", "__iter__", 0, Reference),  // :1165
    ("Config.RoleColorSpaceIterator", "__next__", 0, Reference),  // :1169
    ("Config.DisplayIterator", "__len__", 0, Reference),  // :1177
    ("Config.DisplayIterator", "__getitem__", 0, Reference),  // :1178
    ("Config.DisplayIterator", "__iter__", 0, Reference),  // :1183
    ("Config.DisplayIterator", "__next__", 0, Reference),  // :1184
    ("Config.DisplayAllIterator", "__len__", 0, Reference),  // :1191
    ("Config.DisplayAllIterator", "__getitem__", 0, Reference),  // :1192
    ("Config.DisplayAllIterator", "__iter__", 0, Reference),  // :1197
    ("Config.DisplayAllIterator", "__next__", 0, Reference),  // :1198
    ("Config.SharedViewIterator", "__len__", 0, Reference),  // :1205
    ("Config.SharedViewIterator", "__getitem__", 0, Reference),  // :1207
    ("Config.SharedViewIterator", "__iter__", 0, Reference),  // :1212
    ("Config.SharedViewIterator", "__next__", 0, Reference),  // :1213
    ("Config.VirtualViewIterator", "__len__", 0, Reference),  // :1220
    ("Config.VirtualViewIterator", "__getitem__", 0, Reference),  // :1224
    ("Config.VirtualViewIterator", "__iter__", 0, Reference),  // :1229
    ("Config.VirtualViewIterator", "__next__", 0, Reference),  // :1230
    ("Config.ViewIterator", "__len__", 0, Reference),  // :1237
    ("Config.ViewIterator", "__getitem__", 0, Reference),  // :1239
    ("Config.ViewIterator", "__iter__", 0, Reference),  // :1244
    ("Config.ViewIterator", "__next__", 0, Reference),  // :1245
    ("Config.ViewForColorSpaceIterator", "__len__", 0, Reference),  // :1252
    ("Config.ViewForColorSpaceIterator", "__getitem__", 0, Reference),  // :1255
    ("Config.ViewForColorSpaceIterator", "__iter__", 0, Reference),  // :1262
    ("Config.ViewForColorSpaceIterator", "__next__", 0, Reference),  // :1266
    ("Config.ViewForViewTypeIterator", "__len__", 0, Reference),  // :1275
    ("Config.ViewForViewTypeIterator", "__getitem__", 0, Reference),  // :1278
    ("Config.ViewForViewTypeIterator", "__iter__", 0, Reference),  // :1285
    ("Config.ViewForViewTypeIterator", "__next__", 0, Reference),  // :1289
    ("Config.ActiveDisplaysListIterator", "__len__", 0, Reference),  // :1298
    ("Config.ActiveDisplaysListIterator", "__getitem__", 0, Reference),  // :1299
    ("Config.ActiveDisplaysListIterator", "__iter__", 0, Reference),  // :1304
    ("Config.ActiveDisplaysListIterator", "__next__", 0, Reference),  // :1305
    ("Config.ActiveViewsListIterator", "__len__", 0, Reference),  // :1312
    ("Config.ActiveViewsListIterator", "__getitem__", 0, Reference),  // :1313
    ("Config.ActiveViewsListIterator", "__iter__", 0, Reference),  // :1318
    ("Config.ActiveViewsListIterator", "__next__", 0, Reference),  // :1319
    ("Config.LookNameIterator", "__len__", 0, Reference),  // :1326
    ("Config.LookNameIterator", "__getitem__", 0, Reference),  // :1327
    ("Config.LookNameIterator", "__iter__", 0, Reference),  // :1332
    ("Config.LookNameIterator", "__next__", 0, Reference),  // :1333
    ("Config.LookIterator", "__len__", 0, Reference),  // :1340
    ("Config.LookIterator", "__getitem__", 0, Reference),  // :1341
    ("Config.LookIterator", "__iter__", 0, Reference),  // :1347
    ("Config.LookIterator", "__next__", 0, Reference),  // :1348
    ("Config.ViewTransformNameIterator", "__len__", 0, Reference),  // :1356
    ("Config.ViewTransformNameIterator", "__getitem__", 0, Reference),  // :1360
    ("Config.ViewTransformNameIterator", "__iter__", 0, Reference),  // :1365
    ("Config.ViewTransformNameIterator", "__next__", 0, Reference),  // :1369
    ("Config.ViewTransformIterator", "__len__", 0, Reference),  // :1376
    ("Config.ViewTransformIterator", "__getitem__", 0, Reference),  // :1380
    ("Config.ViewTransformIterator", "__iter__", 0, Reference),  // :1386
    ("Config.ViewTransformIterator", "__next__", 0, Reference),  // :1390
    ("Config.NamedTransformNameIterator", "__len__", 0, Reference),  // :1398
    ("Config.NamedTransformNameIterator", "__getitem__", 0, Reference),  // :1402
    ("Config.NamedTransformNameIterator", "__iter__", 0, Reference),  // :1407
    ("Config.NamedTransformNameIterator", "__next__", 0, Reference),  // :1411
    ("Config.NamedTransformIterator", "__len__", 0, Reference),  // :1418
    ("Config.NamedTransformIterator", "__getitem__", 0, Reference),  // :1422
    ("Config.NamedTransformIterator", "__iter__", 0, Reference),  // :1428
    ("Config.NamedTransformIterator", "__next__", 0, Reference),  // :1432
    ("Config.ActiveNamedTransformNameIterator", "__len__", 0, Reference),  // :1440
    ("Config.ActiveNamedTransformNameIterator", "__getitem__", 0, Reference),  // :1444
    ("Config.ActiveNamedTransformNameIterator", "__iter__", 0, Reference),  // :1449
    ("Config.ActiveNamedTransformNameIterator", "__next__", 0, Reference),  // :1453
    ("Config.ActiveNamedTransformIterator", "__len__", 0, Reference),  // :1460
    ("Config.ActiveNamedTransformIterator", "__getitem__", 0, Reference),  // :1464
    ("Config.ActiveNamedTransformIterator", "__iter__", 0, Reference),  // :1470
    ("Config.ActiveNamedTransformIterator", "__next__", 0, Reference),  // :1474
    // src/bindings/python/PyConfigIOProxy.cpp @ v2.5.2
    ("PyConfigIOProxy", "getLutData", 0, Pointer),  // :60
    ("PyConfigIOProxy", "getConfigData", 0, Pointer),  // :61
    ("PyConfigIOProxy", "getFastLutFileHash", 0, Pointer),  // :62
    // src/bindings/python/PyContext.cpp @ v2.5.2
    ("Context", "__iter__", 0, Iterator),  // :110
    ("Context", "__len__", 0, Pointer),  // :114
    ("Context", "getCacheID", 0, Pointer),  // :149
    ("Context", "getSearchPath", 0, Pointer),  // :151
    ("Context", "getSearchPaths", 0, Iterator),  // :155
    ("Context", "clearSearchPaths", 0, Pointer),  // :159
    ("Context", "getWorkingDir", 0, Pointer),  // :163
    ("Context", "getStringVars", 0, Iterator),  // :167
    ("Context", "clearStringVars", 0, Pointer),  // :173
    ("Context", "getEnvironmentMode", 0, Pointer),  // :175
    ("Context", "loadEnvironment", 0, Pointer),  // :179
    ("Context", "__repr__", 0, Holder),  // :202
    ("Context.StringVarNameIterator", "__len__", 0, Reference),  // :205
    ("Context.StringVarNameIterator", "__getitem__", 0, Reference),  // :209
    ("Context.StringVarNameIterator", "__iter__", 0, Reference),  // :214
    ("Context.StringVarNameIterator", "__next__", 0, Reference),  // :215
    ("Context.StringVarIterator", "__len__", 0, Reference),  // :222
    ("Context.StringVarIterator", "__getitem__", 0, Reference),  // :223
    ("Context.StringVarIterator", "__iter__", 0, Reference),  // :229
    ("Context.StringVarIterator", "__next__", 0, Reference),  // :230
    ("Context.SearchPathIterator", "__len__", 0, Reference),  // :238
    ("Context.SearchPathIterator", "__getitem__", 0, Reference),  // :239
    ("Context.SearchPathIterator", "__iter__", 0, Reference),  // :244
    ("Context.SearchPathIterator", "__next__", 0, Reference),  // :245
    // src/bindings/python/PyDynamicProperty.cpp @ v2.5.2
    ("DynamicProperty", "getType", 0, Pointer),  // :16
    ("DynamicProperty", "getDouble", 0, Pointer),  // :18
    ("DynamicProperty", "getGradingPrimary", 0, Pointer),  // :22
    ("DynamicProperty", "getGradingRGBCurve", 0, Pointer),  // :26
    ("DynamicProperty", "getGradingHueCurve", 0, Pointer),  // :30
    ("DynamicProperty", "getGradingTone", 0, Pointer),  // :34
    // src/bindings/python/PyFileRules.cpp @ v2.5.2
    ("FileRules", "getNumEntries", 0, Pointer),  // :25
    ("FileRules", "isDefault", 0, Pointer),  // :75
    ("FileRules", "__repr__", 0, Holder),  // :77
    // src/bindings/python/PyFormatMetadata.cpp @ v2.5.2
    ("FormatMetadata", "__iter__", 0, Reference),  // :55
    ("FormatMetadata", "__len__", 0, Pointer),  // :59
    ("FormatMetadata", "__repr__", 0, Reference),  // :93
    ("FormatMetadata", "getElementName", 0, Pointer),  // :100
    ("FormatMetadata", "getElementValue", 0, Pointer),  // :104
    ("FormatMetadata", "getAttributes", 0, Reference),  // :108
    ("FormatMetadata", "getChildElements", 0, Reference),  // :112
    ("FormatMetadata", "getName", 0, Pointer),  // :119
    ("FormatMetadata", "getID", 0, Pointer),  // :123
    ("FormatMetadata", "clear", 0, Pointer),  // :127
    ("FormatMetadata.AttributeNameIterator", "__len__", 0, Reference),  // :131
    ("FormatMetadata.AttributeNameIterator", "__getitem__", 0, Reference),  // :132
    ("FormatMetadata.AttributeNameIterator", "__iter__", 0, Reference),  // :137
    ("FormatMetadata.AttributeNameIterator", "__next__", 0, Reference),  // :138
    ("FormatMetadata.AttributeIterator", "__len__", 0, Reference),  // :145
    ("FormatMetadata.AttributeIterator", "__getitem__", 0, Reference),  // :146
    ("FormatMetadata.AttributeIterator", "__iter__", 0, Reference),  // :152
    ("FormatMetadata.AttributeIterator", "__next__", 0, Reference),  // :153
    ("FormatMetadata.ConstChildElementIterator", "__len__", 0, Reference),  // :161
    ("FormatMetadata.ConstChildElementIterator", "__getitem__", 0, Reference),  // :165
    ("FormatMetadata.ConstChildElementIterator", "__iter__", 0, Reference),  // :171
    ("FormatMetadata.ConstChildElementIterator", "__next__", 0, Reference),  // :175
    ("FormatMetadata.ChildElementIterator", "__len__", 0, Reference),  // :183
    ("FormatMetadata.ChildElementIterator", "__getitem__", 0, Reference),  // :187
    ("FormatMetadata.ChildElementIterator", "__iter__", 0, Reference),  // :193
    ("FormatMetadata.ChildElementIterator", "__next__", 0, Reference),  // :194
    // src/bindings/python/PyGPUProcessor.cpp @ v2.5.2
    ("GPUProcessor", "isNoOp", 0, Pointer),  // :15
    ("GPUProcessor", "hasChannelCrosstalk", 0, Pointer),  // :17
    ("GPUProcessor", "getCacheID", 0, Pointer),  // :19
    // src/bindings/python/PyGpuShaderCreator.cpp @ v2.5.2
    ("GpuShaderCreator", "clone", 0, Pointer),  // :43
    ("GpuShaderCreator", "getUniqueID", 0, Pointer),  // :45
    ("GpuShaderCreator", "getLanguage", 0, Pointer),  // :49
    ("GpuShaderCreator", "getFunctionName", 0, Pointer),  // :53
    ("GpuShaderCreator", "getPixelName", 0, Pointer),  // :57
    ("GpuShaderCreator", "getResourcePrefix", 0, Pointer),  // :61
    ("GpuShaderCreator", "getDescriptorSetIndex", 0, Pointer),  // :68
    ("GpuShaderCreator", "getTextureBindingStart", 0, Pointer),  // :70
    ("GpuShaderCreator", "getCacheID", 0, Pointer),  // :72
    ("GpuShaderCreator", "end", 0, Pointer),  // :76
    ("GpuShaderCreator", "getTextureMaxWidth", 0, Pointer),  // :78
    ("GpuShaderCreator", "getAllowTexture1D", 0, Pointer),  // :84
    ("GpuShaderCreator", "getNextResourceIndex", 0, Pointer),  // :86
    ("GpuShaderCreator", "getDynamicProperties", 0, Iterator),  // :98
    ("GpuShaderCreator", "finalize", 0, Pointer),  // :125
    ("GpuShaderCreator.DynamicPropertyIterator", "__len__", 0, Reference),  // :139
    ("GpuShaderCreator.DynamicPropertyIterator", "__getitem__", 0, Reference),  // :143
    ("GpuShaderCreator.DynamicPropertyIterator", "__iter__", 0, Reference),  // :147
    ("GpuShaderCreator.DynamicPropertyIterator", "__next__", 0, Reference),  // :151
    // src/bindings/python/PyGpuShaderDesc.cpp @ v2.5.2
    ("GpuShaderDesc", "clone", 0, Pointer),  // :104
    ("GpuShaderDesc", "getShaderText", 0, Pointer),  // :106
    ("GpuShaderDesc", "getUniforms", 0, Iterator),  // :108
    ("GpuShaderDesc", "getUniformBufferSize", 0, Pointer),  // :112
    ("GpuShaderDesc", "getTextures", 0, Iterator),  // :157
    ("GpuShaderDesc", "get3DTextures", 0, Iterator),  // :184
    ("GpuShaderDesc.UniformData", "getDouble", 0, Reference),  // :196
    ("GpuShaderDesc.UniformData", "getBool", 0, Reference),  // :200
    ("GpuShaderDesc.UniformData", "getFloat3", 0, Reference),  // :204
    ("GpuShaderDesc.UniformData", "getVectorFloat", 0, Reference),  // :208
    ("GpuShaderDesc.UniformData", "getVectorInt", 0, Reference),  // :215
    ("GpuShaderDesc.UniformIterator", "__len__", 0, Reference),  // :224
    ("GpuShaderDesc.UniformIterator", "__getitem__", 0, Reference),  // :228
    ("GpuShaderDesc.UniformIterator", "__iter__", 0, Reference),  // :236
    ("GpuShaderDesc.UniformIterator", "__next__", 0, Reference),  // :240
    ("GpuShaderDesc.Texture", "getValues", 0, Reference),  // :259
    ("GpuShaderDesc.TextureIterator", "__len__", 0, Reference),  // :288
    ("GpuShaderDesc.TextureIterator", "__getitem__", 0, Reference),  // :292
    ("GpuShaderDesc.TextureIterator", "__iter__", 0, Reference),  // :308
    ("GpuShaderDesc.TextureIterator", "__next__", 0, Reference),  // :312
    ("GpuShaderDesc.Texture3D", "getValues", 0, Reference),  // :336
    ("GpuShaderDesc.Texture3DIterator", "__len__", 0, Reference),  // :352
    ("GpuShaderDesc.Texture3DIterator", "__getitem__", 0, Reference),  // :356
    ("GpuShaderDesc.Texture3DIterator", "__iter__", 0, Reference),  // :369
    ("GpuShaderDesc.Texture3DIterator", "__next__", 0, Reference),  // :373
    // src/bindings/python/PyGradingData.cpp @ v2.5.2
    ("GradingRGBM", "__eq__", 0, Reference),  // :85
    ("GradingRGBM", "__ne__", 0, Reference),  // :89
    ("GradingRGBM", "__repr__", 0, Reference),  // :103
    ("GradingPrimary", "__eq__", 0, Reference),  // :109
    ("GradingPrimary", "__ne__", 0, Reference),  // :113
    ("GradingPrimary", "validate", 0, Pointer),  // :118
    ("GradingPrimary", "__repr__", 0, Reference),  // :159
    ("GradingRGBMSW", "__eq__", 0, Reference),  // :171
    ("GradingRGBMSW", "__ne__", 0, Reference),  // :175
    ("GradingRGBMSW", "__repr__", 0, Reference),  // :193
    ("GradingTone", "validate", 0, Pointer),  // :199
    ("GradingTone", "__eq__", 0, Reference),  // :202
    ("GradingTone", "__ne__", 0, Reference),  // :206
    ("GradingTone", "__repr__", 0, Reference),  // :224
    ("GradingControlPoint", "__eq__", 0, Reference),  // :234
    ("GradingControlPoint", "__ne__", 0, Reference),  // :238
    ("GradingControlPoint", "__repr__", 0, Reference),  // :248
    ("GradingBSplineCurve", "__eq__", 0, Reference),  // :319
    ("GradingBSplineCurve", "__ne__", 0, Reference),  // :323
    ("GradingBSplineCurve", "validate", 0, Pointer),  // :328
    ("GradingBSplineCurve", "getControlPoints", 0, Iterator),  // :332
    ("GradingBSplineCurve", "getSplineType", 0, Pointer),  // :336
    ("GradingBSplineCurve", "slopesAreDefault", 0, Pointer),  // :340
    ("GradingBSplineCurve", "getSlopes", 0, Holder),  // :342
    ("GradingBSplineCurve", "setSlopes", 0, Holder),  // :353
    ("GradingBSplineCurve", "__repr__", 0, Holder),  // :367
    ("GradingBSplineCurve.GradingControlPointIterator", "__len__", 0, Reference),  // :370
    ("GradingBSplineCurve.GradingControlPointIterator", "__getitem__", 0, Reference),  // :374
    ("GradingBSplineCurve.GradingControlPointIterator", "__setitem__", 0, Reference),  // :378
    ("GradingBSplineCurve.GradingControlPointIterator", "__iter__", 0, Reference),  // :384
    ("GradingBSplineCurve.GradingControlPointIterator", "__next__", 0, Reference),  // :388
    ("GradingRGBCurve", "__eq__", 0, Reference),  // :415
    ("GradingRGBCurve", "__ne__", 0, Reference),  // :419
    ("GradingRGBCurve", "validate", 0, Pointer),  // :424
    ("GradingRGBCurve", "isIdentity", 0, Pointer),  // :426
    ("GradingHueCurve", "__repr__", 0, Holder),  // :470
    ("GradingHueCurve", "__eq__", 0, Reference),  // :493
    ("GradingHueCurve", "__ne__", 0, Reference),  // :497
    ("GradingHueCurve", "validate", 0, Pointer),  // :502
    ("GradingHueCurve", "isIdentity", 0, Pointer),  // :504
    ("GradingHueCurve", "getDrawCurveOnly", 0, Pointer),  // :506
    ("GradingHueCurve", "__repr__", 1, Holder),  // :592
    // src/bindings/python/PyImageDesc.cpp @ v2.5.2
    ("ImageDesc", "__repr__", 0, Reference),  // :20
    ("ImageDesc", "getBitDepth", 0, Pointer),  // :27
    ("ImageDesc", "getWidth", 0, Pointer),  // :29
    ("ImageDesc", "getHeight", 0, Pointer),  // :31
    ("ImageDesc", "getXStrideBytes", 0, Pointer),  // :33
    ("ImageDesc", "getYStrideBytes", 0, Pointer),  // :35
    ("ImageDesc", "isRGBAPacked", 0, Pointer),  // :37
    ("ImageDesc", "isFloat", 0, Pointer),  // :39
    // src/bindings/python/PyLook.cpp @ v2.5.2
    ("Look", "getName", 0, Pointer),  // :47
    ("Look", "getProcessSpace", 0, Pointer),  // :51
    ("Look", "getTransform", 0, Pointer),  // :55
    ("Look", "getInverseTransform", 0, Pointer),  // :59
    ("Look", "getDescription", 0, Pointer),  // :63
    ("Look", "getInterchangeAttributes", 0, Pointer),  // :71
    ("Look", "__repr__", 0, Holder),  // :74
    // src/bindings/python/PyNamedTransform.cpp @ v2.5.2
    ("NamedTransform", "getName", 0, Pointer),  // :127
    ("NamedTransform", "getAliases", 0, Iterator),  // :139
    ("NamedTransform", "clearAliases", 0, Pointer),  // :143
    ("NamedTransform", "getFamily", 0, Pointer),  // :146
    ("NamedTransform", "getDescription", 0, Pointer),  // :150
    ("NamedTransform", "getEncoding", 0, Pointer),  // :154
    ("NamedTransform", "getCategories", 0, Iterator),  // :175
    ("NamedTransform", "clearCategories", 0, Pointer),  // :179
    ("NamedTransform", "__repr__", 0, Holder),  // :182
    ("NamedTransform.NamedTransformCategoryIterator", "__len__", 0, Reference),  // :185
    ("NamedTransform.NamedTransformCategoryIterator", "__getitem__", 0, Reference),  // :189
    ("NamedTransform.NamedTransformCategoryIterator", "__iter__", 0, Reference),  // :194
    ("NamedTransform.NamedTransformCategoryIterator", "__next__", 0, Reference),  // :198
    ("NamedTransform.NamedTransformAliasIterator", "__len__", 0, Reference),  // :205
    ("NamedTransform.NamedTransformAliasIterator", "__getitem__", 0, Reference),  // :209
    ("NamedTransform.NamedTransformAliasIterator", "__iter__", 0, Reference),  // :214
    ("NamedTransform.NamedTransformAliasIterator", "__next__", 0, Reference),  // :218
    // src/bindings/python/PyPackedImageDesc.cpp @ v2.5.2
    ("PackedImageDesc", "getData", 0, Reference),  // :102
    ("PackedImageDesc", "getChannelOrder", 0, Reference),  // :111
    ("PackedImageDesc", "getNumChannels", 0, Reference),  // :116
    ("PackedImageDesc", "getChanStrideBytes", 0, Reference),  // :121
    // src/bindings/python/PyPlanarImageDesc.cpp @ v2.5.2
    ("PlanarImageDesc", "getRData", 0, Reference),  // :154
    ("PlanarImageDesc", "getGData", 0, Reference),  // :163
    ("PlanarImageDesc", "getBData", 0, Reference),  // :172
    ("PlanarImageDesc", "getAData", 0, Reference),  // :181
    // src/bindings/python/PyProcessor.cpp @ v2.5.2
    ("Processor", "isNoOp", 0, Pointer),  // :34
    ("Processor", "hasChannelCrosstalk", 0, Pointer),  // :36
    ("Processor", "getCacheID", 0, Pointer),  // :38
    ("Processor", "getProcessorMetadata", 0, Pointer),  // :40
    ("Processor", "getFormatMetadata", 0, Pointer),  // :42
    ("Processor", "getTransformFormatMetadata", 0, Iterator),  // :45
    ("Processor", "createGroupTransform", 0, Pointer),  // :49
    ("Processor", "isDynamic", 0, Pointer),  // :62
    ("Processor", "getDefaultGPUProcessor", 0, Pointer),  // :75
    ("Processor", "getDefaultCPUProcessor", 0, Pointer),  // :81
    ("Processor.TransformFormatMetadataIterator", "__len__", 0, Reference),  // :95
    ("Processor.TransformFormatMetadataIterator", "__getitem__", 0, Reference),  // :99
    ("Processor.TransformFormatMetadataIterator", "__iter__", 0, Reference),  // :106
    ("Processor.TransformFormatMetadataIterator", "__next__", 0, Reference),  // :111
    // src/bindings/python/PyProcessorMetadata.cpp @ v2.5.2
    ("ProcessorMetadata", "getFiles", 0, Iterator),  // :44
    ("ProcessorMetadata", "getLooks", 0, Iterator),  // :48
    ("ProcessorMetadata.FileIterator", "__len__", 0, Reference),  // :58
    ("ProcessorMetadata.FileIterator", "__getitem__", 0, Reference),  // :59
    ("ProcessorMetadata.FileIterator", "__iter__", 0, Reference),  // :64
    ("ProcessorMetadata.FileIterator", "__next__", 0, Reference),  // :65
    ("ProcessorMetadata.LookIterator", "__len__", 0, Reference),  // :72
    ("ProcessorMetadata.LookIterator", "__getitem__", 0, Reference),  // :73
    ("ProcessorMetadata.LookIterator", "__iter__", 0, Reference),  // :78
    ("ProcessorMetadata.LookIterator", "__next__", 0, Reference),  // :79
    // src/bindings/python/PySystemMonitors.cpp @ v2.5.2
    ("SystemMonitors", "getMonitors", 0, Reference),  // :36
    ("SystemMonitors.MonitorIterator", "__len__", 0, Reference),  // :42
    ("SystemMonitors.MonitorIterator", "__getitem__", 0, Reference),  // :43
    ("SystemMonitors.MonitorIterator", "__iter__", 0, Reference),  // :49
    ("SystemMonitors.MonitorIterator", "__next__", 0, Reference),  // :50
    // src/bindings/python/PyTransform.cpp @ v2.5.2
    ("Transform", "validate", 0, Pointer),  // :22
    ("Transform", "getTransformType", 0, Pointer),  // :24
    ("Transform", "getDirection", 0, Pointer),  // :26
    ("Transform", "__repr__", 0, Holder),  // :31
    // src/bindings/python/PyViewTransform.cpp @ v2.5.2
    ("ViewTransform", "getName", 0, Pointer),  // :96
    ("ViewTransform", "getFamily", 0, Pointer),  // :100
    ("ViewTransform", "getDescription", 0, Pointer),  // :104
    ("ViewTransform", "getInterchangeAttributes", 0, Pointer),  // :112
    ("ViewTransform", "getCategories", 0, Iterator),  // :120
    ("ViewTransform", "clearCategories", 0, Pointer),  // :124
    ("ViewTransform", "getReferenceSpaceType", 0, Pointer),  // :126
    ("ViewTransform", "__repr__", 0, Holder),  // :133
    ("ViewTransform.ViewTransformCategoryIterator", "__len__", 0, Reference),  // :136
    ("ViewTransform.ViewTransformCategoryIterator", "__getitem__", 0, Reference),  // :140
    ("ViewTransform.ViewTransformCategoryIterator", "__iter__", 0, Reference),  // :145
    ("ViewTransform.ViewTransformCategoryIterator", "__next__", 0, Reference),  // :149
    // src/bindings/python/PyViewingRules.cpp @ v2.5.2
    ("ViewingRules", "getNumEntries", 0, Pointer),  // :43
    ("ViewingRules", "__repr__", 0, Holder),  // :85
    ("ViewingRules.ViewingRuleColorSpaceIterator", "__len__", 0, Reference),  // :88
    ("ViewingRules.ViewingRuleColorSpaceIterator", "__getitem__", 0, Reference),  // :92
    ("ViewingRules.ViewingRuleColorSpaceIterator", "__iter__", 0, Reference),  // :99
    ("ViewingRules.ViewingRuleColorSpaceIterator", "__next__", 0, Reference),  // :103
    ("ViewingRules.ViewingRuleEncodingIterator", "__len__", 0, Reference),  // :111
    ("ViewingRules.ViewingRuleEncodingIterator", "__getitem__", 0, Reference),  // :115
    ("ViewingRules.ViewingRuleEncodingIterator", "__iter__", 0, Reference),  // :122
    ("ViewingRules.ViewingRuleEncodingIterator", "__next__", 0, Reference),  // :126
    // src/bindings/python/apphelpers/PyColorSpaceHelpers.cpp @ v2.5.2
    ("ColorSpaceMenuParameters", "getConfig", 0, Pointer),  // :109
    ("ColorSpaceMenuParameters", "getRole", 0, Pointer),  // :113
    ("ColorSpaceMenuParameters", "getIncludeColorSpaces", 0, Pointer),  // :117
    ("ColorSpaceMenuParameters", "getSearchReferenceSpaceType", 0, Pointer),  // :122
    ("ColorSpaceMenuParameters", "getIncludeNamedTransforms", 0, Pointer),  // :127
    ("ColorSpaceMenuParameters", "getTreatNoCategoryAsAny", 0, Pointer),  // :132
    ("ColorSpaceMenuParameters", "getEncodings", 0, Pointer),  // :137
    ("ColorSpaceMenuParameters", "getAppCategories", 0, Pointer),  // :141
    ("ColorSpaceMenuParameters", "getUserCategories", 0, Pointer),  // :146
    ("ColorSpaceMenuParameters", "getIncludeRoles", 0, Pointer),  // :151
    ("ColorSpaceMenuParameters", "getAddedColorSpaces", 0, Iterator),  // :159
    ("ColorSpaceMenuParameters", "clearAddedColorSpaces", 0, Pointer),  // :163
    ("ColorSpaceMenuParameters", "__repr__", 0, Holder),  // :166
    ("ColorSpaceMenuParameters.AddedColorSpaceIterator", "__len__", 0, Reference),  // :169
    ("ColorSpaceMenuParameters.AddedColorSpaceIterator", "__getitem__", 0, Reference),  // :173
    ("ColorSpaceMenuParameters.AddedColorSpaceIterator", "__iter__", 0, Reference),  // :178
    ("ColorSpaceMenuParameters.AddedColorSpaceIterator", "__next__", 0, Reference),  // :182
    ("ColorSpaceMenuHelper", "getNumColorSpaces", 0, Pointer),  // :191
    ("ColorSpaceMenuHelper", "__repr__", 0, Holder),  // :216
    ("ColorSpaceMenuHelper.ColorSpaceLevelIterator", "__len__", 0, Reference),  // :219
    ("ColorSpaceMenuHelper.ColorSpaceLevelIterator", "__getitem__", 0, Reference),  // :223
    ("ColorSpaceMenuHelper.ColorSpaceLevelIterator", "__iter__", 0, Reference),  // :228
    ("ColorSpaceMenuHelper.ColorSpaceLevelIterator", "__next__", 0, Reference),  // :232
    // src/bindings/python/apphelpers/PyLegacyViewingPipeline.cpp @ v2.5.2
    ("LegacyViewingPipeline", "getDisplayViewTransform", 0, Pointer),  // :18
    ("LegacyViewingPipeline", "setDisplayViewTransform", 0, Pointer),  // :20
    ("LegacyViewingPipeline", "getLinearCC", 0, Pointer),  // :22
    ("LegacyViewingPipeline", "setLinearCC", 0, Pointer),  // :24
    ("LegacyViewingPipeline", "getColorTimingCC", 0, Pointer),  // :26
    ("LegacyViewingPipeline", "setColorTimingCC", 0, Pointer),  // :28
    ("LegacyViewingPipeline", "getChannelView", 0, Pointer),  // :30
    ("LegacyViewingPipeline", "setChannelView", 0, Pointer),  // :32
    ("LegacyViewingPipeline", "getDisplayCC", 0, Pointer),  // :34
    ("LegacyViewingPipeline", "setDisplayCC", 0, Pointer),  // :36
    ("LegacyViewingPipeline", "setLooksOverrideEnabled", 0, Pointer),  // :38
    ("LegacyViewingPipeline", "getLooksOverrideEnabled", 0, Pointer),  // :40
    ("LegacyViewingPipeline", "getLooksOverride", 0, Pointer),  // :44
    ("LegacyViewingPipeline", "__repr__", 0, Holder),  // :57
    // src/bindings/python/apphelpers/PyMergeConfigs.cpp @ v2.5.2
    ("ConfigMergingParameters", "getBaseConfigName", 0, Pointer),  // :34
    ("ConfigMergingParameters", "getInputConfigName", 0, Pointer),  // :38
    ("ConfigMergingParameters", "getOutputName", 0, Pointer),  // :42
    ("ConfigMergingParameters", "getDefaultStrategy", 0, Pointer),  // :46
    ("ConfigMergingParameters", "getInputFamilyPrefix", 0, Pointer),  // :50
    ("ConfigMergingParameters", "getBaseFamilyPrefix", 0, Pointer),  // :54
    ("ConfigMergingParameters", "isInputFirst", 0, Pointer),  // :58
    ("ConfigMergingParameters", "isErrorOnConflict", 0, Pointer),  // :62
    ("ConfigMergingParameters", "isAvoidDuplicates", 0, Pointer),  // :66
    ("ConfigMergingParameters", "isAdjustInputReferenceSpace", 0, Pointer),  // :70
    ("ConfigMergingParameters", "getName", 0, Pointer),  // :74
    ("ConfigMergingParameters", "getDescription", 0, Pointer),  // :78
    ("ConfigMergingParameters", "getNumEnvironmentVars", 0, Pointer),  // :83
    ("ConfigMergingParameters", "getSearchPath", 0, Pointer),  // :94
    ("ConfigMergingParameters", "getActiveDisplays", 0, Pointer),  // :99
    ("ConfigMergingParameters", "getActiveViews", 0, Pointer),  // :103
    ("ConfigMergingParameters", "getInactiveColorSpaces", 0, Pointer),  // :107
    ("ConfigMergingParameters", "getRoles", 0, Pointer),  // :112
    ("ConfigMergingParameters", "getFileRules", 0, Pointer),  // :116
    ("ConfigMergingParameters", "getDisplayViews", 0, Pointer),  // :120
    ("ConfigMergingParameters", "getViewTransforms", 0, Pointer),  // :124
    ("ConfigMergingParameters", "getLooks", 0, Pointer),  // :128
    ("ConfigMergingParameters", "getColorspaces", 0, Pointer),  // :132
    ("ConfigMergingParameters", "getNamedTransforms", 0, Pointer),  // :136
    ("ConfigMergingParameters", "__repr__", 0, Holder),  // :154
    ("ConfigMerger", "getNumSearchPaths", 0, Pointer),  // :175
    ("ConfigMerger", "getWorkingDir", 0, Pointer),  // :182
    ("ConfigMerger", "getNumConfigMergingParameters", 0, Pointer),  // :186
    ("ConfigMerger", "mergeConfigs", 0, Pointer),  // :190
    ("ConfigMerger", "getNumMergedConfigs", 0, Pointer),  // :192
    ("ConfigMerger", "getMergedConfig", 0, Pointer),  // :194
    ("ConfigMerger", "serialize", 0, Reference),  // :201
    ("ConfigMerger", "getMajorVersion", 0, Pointer),  // :208
    ("ConfigMerger", "getMinorVersion", 0, Pointer),  // :210
    ("ConfigMerger", "__repr__", 0, Holder),  // :213
    // src/bindings/python/apphelpers/PyMixingHelpers.cpp @ v2.5.2
    ("MixingSlider", "setSliderMinEdge", 0, Pointer),  // :45
    ("MixingSlider", "getSliderMinEdge", 0, Pointer),  // :47
    ("MixingSlider", "setSliderMaxEdge", 0, Pointer),  // :49
    ("MixingSlider", "getSliderMaxEdge", 0, Pointer),  // :51
    ("MixingSlider", "__repr__", 0, Reference),  // :58
    ("MixingColorSpaceManager", "getMixingSpaces", 0, Iterator),  // :67
    ("MixingColorSpaceManager", "setSelectedMixingSpaceIdx", 0, Pointer),  // :71
    ("MixingColorSpaceManager", "getSelectedMixingSpaceIdx", 0, Pointer),  // :76
    ("MixingColorSpaceManager", "isPerceptuallyUniform", 0, Pointer),  // :78
    ("MixingColorSpaceManager", "getMixingEncodings", 0, Iterator),  // :80
    ("MixingColorSpaceManager", "setSelectedMixingEncodingIdx", 0, Pointer),  // :84
    ("MixingColorSpaceManager", "getSelectedMixingEncodingIdx", 0, Pointer),  // :89
    ("MixingColorSpaceManager", "getSlider", 0, Pointer),  // :99
    ("MixingColorSpaceManager", "__repr__", 0, Holder),  // :111
    ("MixingColorSpaceManager.MixingSpaceIterator", "__len__", 0, Reference),  // :114
    ("MixingColorSpaceManager.MixingSpaceIterator", "__getitem__", 0, Reference),  // :118
    ("MixingColorSpaceManager.MixingSpaceIterator", "__iter__", 0, Reference),  // :123
    ("MixingColorSpaceManager.MixingSpaceIterator", "__next__", 0, Reference),  // :127
    ("MixingColorSpaceManager.MixingEncodingIterator", "__len__", 0, Reference),  // :134
    ("MixingColorSpaceManager.MixingEncodingIterator", "__getitem__", 0, Reference),  // :138
    ("MixingColorSpaceManager.MixingEncodingIterator", "__iter__", 0, Reference),  // :143
    ("MixingColorSpaceManager.MixingEncodingIterator", "__next__", 0, Reference),  // :147
    // pybind11 stl_bind.h (bind_vector)
    ("vector_of_uint8_t", "__bool__", 0, Reference),
    ("vector_of_uint8_t", "__delitem__", 0, Reference),
    ("vector_of_uint8_t", "__delitem__", 1, Reference),
    ("vector_of_uint8_t", "__eq__", 0, Reference),
    ("vector_of_uint8_t", "__getitem__", 1, Reference),
    ("vector_of_uint8_t", "__iter__", 0, Reference),
    ("vector_of_uint8_t", "__len__", 0, Reference),
    ("vector_of_uint8_t", "__ne__", 0, Reference),
    ("vector_of_uint8_t", "__repr__", 0, Reference),
    ("vector_of_uint8_t", "__setitem__", 0, Reference),
    ("vector_of_uint8_t", "__setitem__", 1, Reference),
    ("vector_of_uint8_t", "clear", 0, Reference),
    ("vector_of_uint8_t", "pop", 0, Reference),
    // src/bindings/python/transforms/PyAllocationTransform.cpp @ v2.5.2
    ("AllocationTransform", "getAllocation", 0, Pointer),  // :56
    ("AllocationTransform", "getVars", 0, Holder),  // :61
    ("AllocationTransform", "__repr__", 0, Holder),  // :73
    // src/bindings/python/transforms/PyBuiltinTransform.cpp @ v2.5.2
    ("BuiltinTransform", "getStyle", 0, Pointer),  // :33
    ("BuiltinTransform", "getDescription", 0, Pointer),  // :35
    ("BuiltinTransform", "__repr__", 0, Holder),  // :38
    // src/bindings/python/transforms/PyCDLTransform.cpp @ v2.5.2
    ("CDLTransform", "getFormatMetadata", 0, Pointer),  // :73
    ("CDLTransform", "getStyle", 0, Pointer),  // :81
    ("CDLTransform", "getSlope", 0, Holder),  // :83
    ("CDLTransform", "getOffset", 0, Holder),  // :96
    ("CDLTransform", "getPower", 0, Holder),  // :109
    ("CDLTransform", "getSOP", 0, Holder),  // :122
    ("CDLTransform", "getSat", 0, Pointer),  // :135
    ("CDLTransform", "getSatLumaCoefs", 0, Holder),  // :139
    ("CDLTransform", "getID", 0, Pointer),  // :146
    ("CDLTransform", "getFirstSOPDescription", 0, Pointer),  // :150
    ("CDLTransform", "__repr__", 0, Holder),  // :156
    // src/bindings/python/transforms/PyColorSpaceTransform.cpp @ v2.5.2
    ("ColorSpaceTransform", "getSrc", 0, Pointer),  // :38
    ("ColorSpaceTransform", "getDst", 0, Pointer),  // :42
    ("ColorSpaceTransform", "getDataBypass", 0, Pointer),  // :46
    ("ColorSpaceTransform", "__repr__", 0, Holder),  // :51
    // src/bindings/python/transforms/PyDisplayViewTransform.cpp @ v2.5.2
    ("DisplayViewTransform", "getSrc", 0, Pointer),  // :44
    ("DisplayViewTransform", "getDisplay", 0, Pointer),  // :48
    ("DisplayViewTransform", "getView", 0, Pointer),  // :52
    ("DisplayViewTransform", "getLooksBypass", 0, Pointer),  // :56
    ("DisplayViewTransform", "getDataBypass", 0, Pointer),  // :60
    ("DisplayViewTransform", "__repr__", 0, Holder),  // :65
    // src/bindings/python/transforms/PyExponentTransform.cpp @ v2.5.2
    ("ExponentTransform", "getFormatMetadata", 0, Pointer),  // :38
    ("ExponentTransform", "getValue", 0, Holder),  // :44
    ("ExponentTransform", "getNegativeStyle", 0, Pointer),  // :57
    ("ExponentTransform", "__repr__", 0, Holder),  // :62
    // src/bindings/python/transforms/PyExponentWithLinearTransform.cpp @ v2.5.2
    ("ExponentWithLinearTransform", "getFormatMetadata", 0, Pointer),  // :44
    ("ExponentWithLinearTransform", "getGamma", 0, Holder),  // :51
    ("ExponentWithLinearTransform", "getOffset", 0, Holder),  // :65
    ("ExponentWithLinearTransform", "getNegativeStyle", 0, Pointer),  // :79
    ("ExponentWithLinearTransform", "__repr__", 0, Holder),  // :84
    // src/bindings/python/transforms/PyExposureContrastTransform.cpp @ v2.5.2
    ("ExposureContrastTransform", "getFormatMetadata", 0, Pointer),  // :59
    ("ExposureContrastTransform", "getStyle", 0, Pointer),  // :66
    ("ExposureContrastTransform", "getExposure", 0, Pointer),  // :70
    ("ExposureContrastTransform", "isExposureDynamic", 0, Pointer),  // :74
    ("ExposureContrastTransform", "makeExposureDynamic", 0, Pointer),  // :76
    ("ExposureContrastTransform", "makeExposureNonDynamic", 0, Pointer),  // :78
    ("ExposureContrastTransform", "getContrast", 0, Pointer),  // :80
    ("ExposureContrastTransform", "isContrastDynamic", 0, Pointer),  // :84
    ("ExposureContrastTransform", "makeContrastDynamic", 0, Pointer),  // :86
    ("ExposureContrastTransform", "makeContrastNonDynamic", 0, Pointer),  // :88
    ("ExposureContrastTransform", "getGamma", 0, Pointer),  // :90
    ("ExposureContrastTransform", "isGammaDynamic", 0, Pointer),  // :94
    ("ExposureContrastTransform", "makeGammaDynamic", 0, Pointer),  // :96
    ("ExposureContrastTransform", "makeGammaNonDynamic", 0, Pointer),  // :98
    ("ExposureContrastTransform", "getPivot", 0, Pointer),  // :100
    ("ExposureContrastTransform", "getLogExposureStep", 0, Pointer),  // :104
    ("ExposureContrastTransform", "getLogMidGray", 0, Pointer),  // :109
    ("ExposureContrastTransform", "__repr__", 0, Holder),  // :114
    // src/bindings/python/transforms/PyFileTransform.cpp @ v2.5.2
    ("FileTransform", "getSrc", 0, Pointer),  // :60
    ("FileTransform", "getCCCId", 0, Pointer),  // :64
    ("FileTransform", "getCDLStyle", 0, Pointer),  // :68
    ("FileTransform", "getInterpolation", 0, Pointer),  // :72
    ("FileTransform", "__repr__", 0, Holder),  // :79
    ("FileTransform.FormatIterator", "__len__", 0, Reference),  // :82
    ("FileTransform.FormatIterator", "__getitem__", 0, Reference),  // :86
    ("FileTransform.FormatIterator", "__iter__", 0, Reference),  // :92
    ("FileTransform.FormatIterator", "__next__", 0, Reference),  // :96
    // src/bindings/python/transforms/PyFixedFunctionTransform.cpp @ v2.5.2
    ("FixedFunctionTransform", "getFormatMetadata", 0, Pointer),  // :44
    ("FixedFunctionTransform", "getStyle", 0, Pointer),  // :51
    ("FixedFunctionTransform", "getParams", 0, Holder),  // :55
    ("FixedFunctionTransform", "__repr__", 0, Holder),  // :67
    // src/bindings/python/transforms/PyGradingHueCurveTransform.cpp @ v2.5.2
    ("GradingHueCurveTransform", "getFormatMetadata", 0, Pointer),  // :52
    ("GradingHueCurveTransform", "getStyle", 0, Pointer),  // :57
    ("GradingHueCurveTransform", "getValue", 0, Pointer),  // :61
    ("GradingHueCurveTransform", "getRGBToHSY", 0, Pointer),  // :101
    ("GradingHueCurveTransform", "isDynamic", 0, Pointer),  // :105
    ("GradingHueCurveTransform", "makeDynamic", 0, Pointer),  // :107
    ("GradingHueCurveTransform", "makeNonDynamic", 0, Pointer),  // :109
    ("GradingHueCurveTransform", "__repr__", 0, Holder),  // :112
    // src/bindings/python/transforms/PyGradingPrimaryTransform.cpp @ v2.5.2
    ("GradingPrimaryTransform", "getFormatMetadata", 0, Pointer),  // :52
    ("GradingPrimaryTransform", "getStyle", 0, Pointer),  // :57
    ("GradingPrimaryTransform", "getValue", 0, Pointer),  // :61
    ("GradingPrimaryTransform", "isDynamic", 0, Pointer),  // :65
    ("GradingPrimaryTransform", "makeDynamic", 0, Pointer),  // :67
    ("GradingPrimaryTransform", "makeNonDynamic", 0, Pointer),  // :69
    ("GradingPrimaryTransform", "__repr__", 0, Holder),  // :72
    // src/bindings/python/transforms/PyGradingRGBCurveTransform.cpp @ v2.5.2
    ("GradingRGBCurveTransform", "getFormatMetadata", 0, Pointer),  // :52
    ("GradingRGBCurveTransform", "getStyle", 0, Pointer),  // :57
    ("GradingRGBCurveTransform", "getValue", 0, Pointer),  // :61
    ("GradingRGBCurveTransform", "getBypassLinToLog", 0, Pointer),  // :71
    ("GradingRGBCurveTransform", "isDynamic", 0, Pointer),  // :75
    ("GradingRGBCurveTransform", "makeDynamic", 0, Pointer),  // :77
    ("GradingRGBCurveTransform", "makeNonDynamic", 0, Pointer),  // :79
    ("GradingRGBCurveTransform", "__repr__", 0, Holder),  // :82
    // src/bindings/python/transforms/PyGradingToneTransform.cpp @ v2.5.2
    ("GradingToneTransform", "getFormatMetadata", 0, Pointer),  // :52
    ("GradingToneTransform", "getStyle", 0, Pointer),  // :57
    ("GradingToneTransform", "getValue", 0, Pointer),  // :61
    ("GradingToneTransform", "isDynamic", 0, Pointer),  // :65
    ("GradingToneTransform", "makeDynamic", 0, Pointer),  // :67
    ("GradingToneTransform", "makeNonDynamic", 0, Pointer),  // :69
    ("GradingToneTransform", "__repr__", 0, Holder),  // :72
    // src/bindings/python/transforms/PyGroupTransform.cpp @ v2.5.2
    ("GroupTransform", "__iter__", 0, Iterator),  // :65
    ("GroupTransform", "__len__", 0, Pointer),  // :69
    ("GroupTransform", "getFormatMetadata", 0, Pointer),  // :76
    ("GroupTransform", "__repr__", 0, Holder),  // :126
    ("GroupTransform.TransformIterator", "__len__", 0, Reference),  // :129
    ("GroupTransform.TransformIterator", "__getitem__", 0, Reference),  // :133
    ("GroupTransform.TransformIterator", "__iter__", 0, Reference),  // :138
    ("GroupTransform.TransformIterator", "__next__", 0, Reference),  // :142
    ("GroupTransform.WriteFormatIterator", "__len__", 0, Reference),  // :149
    ("GroupTransform.WriteFormatIterator", "__getitem__", 0, Reference),  // :153
    ("GroupTransform.WriteFormatIterator", "__iter__", 0, Reference),  // :159
    ("GroupTransform.WriteFormatIterator", "__next__", 0, Reference),  // :163
    // src/bindings/python/transforms/PyLogAffineTransform.cpp @ v2.5.2
    ("LogAffineTransform", "getFormatMetadata", 0, Pointer),  // :53
    ("LogAffineTransform", "getBase", 0, Pointer),  // :59
    ("LogAffineTransform", "getLogSideSlopeValue", 0, Holder),  // :63
    ("LogAffineTransform", "getLogSideOffsetValue", 0, Holder),  // :77
    ("LogAffineTransform", "getLinSideSlopeValue", 0, Holder),  // :91
    ("LogAffineTransform", "getLinSideOffsetValue", 0, Holder),  // :105
    ("LogAffineTransform", "__repr__", 0, Holder),  // :120
    // src/bindings/python/transforms/PyLogCameraTransform.cpp @ v2.5.2
    ("LogCameraTransform", "getFormatMetadata", 0, Pointer),  // :74
    ("LogCameraTransform", "getBase", 0, Pointer),  // :80
    ("LogCameraTransform", "getLogSideSlopeValue", 0, Holder),  // :84
    ("LogCameraTransform", "getLogSideOffsetValue", 0, Holder),  // :98
    ("LogCameraTransform", "getLinSideSlopeValue", 0, Holder),  // :112
    ("LogCameraTransform", "getLinSideOffsetValue", 0, Holder),  // :126
    ("LogCameraTransform", "getLinSideBreakValue", 0, Holder),  // :140
    ("LogCameraTransform", "getLinearSlopeValue", 0, Holder),  // :156
    ("LogCameraTransform", "isLinearSlopeValueSet", 0, Holder),  // :174
    ("LogCameraTransform", "unsetLinearSlopeValue", 0, Pointer),  // :179
    ("LogCameraTransform", "__repr__", 0, Holder),  // :182
    // src/bindings/python/transforms/PyLogTransform.cpp @ v2.5.2
    ("LogTransform", "getFormatMetadata", 0, Pointer),  // :32
    ("LogTransform", "getBase", 0, Pointer),  // :38
    ("LogTransform", "__repr__", 0, Holder),  // :43
    // src/bindings/python/transforms/PyLookTransform.cpp @ v2.5.2
    ("LookTransform", "getSrc", 0, Pointer),  // :41
    ("LookTransform", "getDst", 0, Pointer),  // :45
    ("LookTransform", "getLooks", 0, Pointer),  // :49
    ("LookTransform", "getSkipColorSpaceConversion", 0, Pointer),  // :53
    ("LookTransform", "__repr__", 0, Holder),  // :59
    // src/bindings/python/transforms/PyLut1DTransform.cpp @ v2.5.2
    ("Lut1DTransform", "getFileOutputBitDepth", 0, Pointer),  // :54
    ("Lut1DTransform", "getFormatMetadata", 0, Pointer),  // :58
    ("Lut1DTransform", "getLength", 0, Pointer),  // :64
    ("Lut1DTransform", "getData", 0, Holder),  // :97
    ("Lut1DTransform", "getInputHalfDomain", 0, Pointer),  // :120
    ("Lut1DTransform", "getOutputRawHalfs", 0, Pointer),  // :124
    ("Lut1DTransform", "getHueAdjust", 0, Pointer),  // :128
    ("Lut1DTransform", "getInterpolation", 0, Pointer),  // :132
    ("Lut1DTransform", "__repr__", 0, Holder),  // :137
    // src/bindings/python/transforms/PyLut3DTransform.cpp @ v2.5.2
    ("Lut3DTransform", "getFileOutputBitDepth", 0, Pointer),  // :48
    ("Lut3DTransform", "getFormatMetadata", 0, Pointer),  // :52
    ("Lut3DTransform", "getGridSize", 0, Pointer),  // :58
    ("Lut3DTransform", "getData", 0, Holder),  // :105
    ("Lut3DTransform", "getInterpolation", 0, Pointer),  // :135
    ("Lut3DTransform", "__repr__", 0, Holder),  // :140
    // src/bindings/python/transforms/PyMatrixTransform.cpp @ v2.5.2
    ("MatrixTransform", "getFormatMetadata", 0, Pointer),  // :116
    ("MatrixTransform", "getMatrix", 0, Holder),  // :122
    ("MatrixTransform", "getOffset", 0, Holder),  // :135
    ("MatrixTransform", "getFileInputBitDepth", 0, Pointer),  // :148
    ("MatrixTransform", "getFileOutputBitDepth", 0, Pointer),  // :152
    ("MatrixTransform", "__repr__", 0, Holder),  // :157
    // src/bindings/python/transforms/PyRangeTransform.cpp @ v2.5.2
    ("RangeTransform", "getStyle", 0, Pointer),  // :41
    ("RangeTransform", "getFormatMetadata", 0, Pointer),  // :45
    ("RangeTransform", "getFileInputBitDepth", 0, Pointer),  // :51
    ("RangeTransform", "getFileOutputBitDepth", 0, Pointer),  // :55
    ("RangeTransform", "getMinInValue", 0, Pointer),  // :59
    ("RangeTransform", "hasMinInValue", 0, Pointer),  // :63
    ("RangeTransform", "unsetMinInValue", 0, Pointer),  // :65
    ("RangeTransform", "getMaxInValue", 0, Pointer),  // :67
    ("RangeTransform", "hasMaxInValue", 0, Pointer),  // :71
    ("RangeTransform", "unsetMaxInValue", 0, Pointer),  // :73
    ("RangeTransform", "getMinOutValue", 0, Pointer),  // :75
    ("RangeTransform", "hasMinOutValue", 0, Pointer),  // :79
    ("RangeTransform", "unsetMinOutValue", 0, Pointer),  // :81
    ("RangeTransform", "getMaxOutValue", 0, Pointer),  // :83
    ("RangeTransform", "hasMaxOutValue", 0, Pointer),  // :87
    ("RangeTransform", "unsetMaxOutValue", 0, Pointer),  // :89
    ("RangeTransform", "__repr__", 0, Holder),  // :92
];
