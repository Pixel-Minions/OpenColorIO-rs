# wheel-inspect: reading the wheel's machine code

Some of what the port must reproduce is decided by how the official wheel was *compiled*, not
by the C++ source: the operand order where two NaNs meet, float or double arithmetic, calls to
libm or SVML, fused multiply-adds. `tools/wheel-inspect` reads both pinned libraries,
`OpenColorIO_2_5.dll` (Windows, MSVC) and `libOpenColorIO.so` (Linux, GCC), and answers these
questions in a few commands. The worked examples below are the spikes' findings
(`docs/spikes/s2-s5.md`, `docs/spikes/s4.md`); `wheel-inspect selftest` reproduces all of them.

## Running it

```sh
uv run --project tools/wheel-inspect wheel-inspect <command>   # Windows, from the checkout
tools/wheel-inspect/rocky.sh <command>                         # Rocky Linux 9 (scripts/rocky9.sh)
```

- **Both libraries on both platforms.** The first run downloads the two wheels that
  `oracle/uv.lock` pins, from the URLs in the lock and nowhere else, into
  `<target>/wheel-inspect/`. Every run checks their SHA-256 against the lock, and the
  extracted libraries against the wheels' `RECORD`. `locate` shows where everything is and
  whether the oracle environment's copy is the same file.
- **Addresses** are the ones the spikes cite: Windows virtual addresses with the image base
  (`0x180210d18`), Linux addresses as in the `.so` (`0x3f26c0`).
- **Output is stable text**: Intel syntax (`--att` for objdump's AT&T), one instruction per
  line, with calls, imports, vtables and read-only constants annotated after `;`. The header
  names the library's SHA-256 and the tool versions.

| Command | What it does |
|---|---|
| `locate [--names]` | The pinned wheels and libraries, verified; `--names` counts the names the DLL gets |
| `find NAME` | The function in each build. `NAME` is a C++ name or a `::`-suffix of one: `GetLogSideBreak`, `CameraLog2LinRenderer::apply`, `Renderer_LIN_TO_PQ_SSE<false>::apply` |
| `disasm NAME` or `disasm 0xADDR` | The function's disassembly (by address: the function holding it, `>` marks the address; `--around N` keeps N instructions either side) |
| `imports [--grep RE] [--all]` | The math library and SVML imports (`--all`: every import); with `--grep`, each call site, grouped by function |
| `scan RE [--list]` | Instructions whose mnemonic matches `RE`, counted per function |
| `selftest` | Reproduces the spikes' findings; any mismatch fails |

`--win` or `--linux` limits a command to one library.

## Reading operand order

- Intel syntax puts the destination first. SSE is two-operand: `addss xmm1, dword ptr [rbx + 0xbc]`
  is `xmm1 = xmm1 + [rbx + 0xbc]`, so `xmm1` is the **first** operand. AVX is three-operand:
  `vaddss xmm0, xmm1, xmm2` is `xmm0 = xmm1 + xmm2`, first operand `xmm1`.
- When both operands are NaN, `addss`/`mulss`/`addps`/`mulps` (and `sd`/`pd`) return the first
  operand's NaN, quieted. That is why the order matters (CLAUDE.md, "Bit-exact porting").
- FMA: `vfmadd132ps a, b, c` is `a*c + b`, `213` is `b*a + c`, `231` is `b*c + a`. With several
  NaN operands the result depends on the form (s4.md, "Mismatches and their causes").

## Which operand order did the compiler use at `X.cpp:N`?

Example: `LogOpCPU.cpp:787`, `m_linsinv[i] * (in[i] + m_minuslino[i])` in
`CameraLog2LinRenderer::apply`.

1. **Find the function in both builds.**
   ```
   $ wheel-inspect find CameraLog2LinRenderer::apply
   == windows: OpenColorIO_2_5.dll (sha256 f75646b44d33f947...)
     0x180210cd0-0x180210e48  OpenColorIO_v2_5::CameraLog2LinRenderer::apply(void const*, void*, long) const  [.pdata, 2 chained entries; vtable 0x1803e8048 of OpenColorIO_v2_5::CameraLog2LinRenderer, slot 1 (Linux slot 2)]
   == linux: libOpenColorIO.so (sha256 18c4d7ad40543ae0...)
     0x3f2690-0x3f282e  OpenColorIO_v2_5::CameraLog2LinRenderer::apply(void const*, void*, long) const  [symbol]
   ```
2. **Find the source line's arithmetic.** `disasm CameraLog2LinRenderer::apply` prints both.
   Member offsets identify the operands: after the vtable pointer, `m_base` and the three
   parameter vectors, `m_logSideBreak` is at `0x58`, and the renderer's six `float[3]` arrays
   follow at `0x80`: `m_kinv`, `m_minuskb` (`0x8c`), `m_minusb`, `m_minv`, `m_linsinv` (`0xb0`),
   `m_minuslino` (`0xbc`). The `comiss` against `[rbx + 0x58]` is the break test, and the branch
   below it is the linear segment:
   ```
   $ wheel-inspect disasm 0x180210d18 --around 3
     180210d0d:  comiss   xmm0, xmm1
     180210d10:  movss    xmm6, dword ptr [rdi + rsi + 8]
     180210d16:  jbe      0x180210d2a
   > 180210d18:  addss    xmm1, dword ptr [rbx + 0xbc]
     180210d20:  mulss    xmm1, dword ptr [rbx + 0xb0]
     180210d28:  jmp      0x180210d60
     180210d2a:  addss    xmm1, dword ptr [rbx + 0x8c]
   ```
3. **Read the order.** `xmm1` holds `in`: the sum is `in + minuslino`, and the product's first
   operand is that sum, so the machine computes `(in + minuslino) * linsinv`, the reverse of the
   source's product. Linux has the same at `0x3f26c0` (`addss xmm0, [rbx + 0xbc]; mulss xmm0,
   [rbx + 0xb0]`). The port writes `sse_mul(sse_add(in, minuslino), linsinv)`.

The other sites of s2-s5.md's table are checked the same way by the selftest: `GetLinearOffset`
(`0x18021ab8c`, `0x40185f`), `GetLinearSlope` (`0x18021abf1`, `0x40178b`) and
`GammaMoncurveMirrorOpCPUFwd::apply` (`0x1801bdeb4`, `0x384ea5`).

## Does this path call libm, SVML or an FMA?

- **Calls from one function:**
  ```
  $ wheel-inspect disasm 'Renderer_LIN_TO_PQ_SSE<false>::apply' --win | grep call
    18018a301:  call     0x18034b0b0      ; __vdecl_powf4 (SVML, linked statically)
    ...
  $ wheel-inspect disasm 'Renderer_LIN_TO_PQ<float>::apply' --linux | grep call
    35b06a:  call     0x91500             ; powf@GLIBC_2.27 (libm.so.6)
  ```
  With fast math off, the Windows PQ kernel calls MSVC's SVML `__vdecl_powf4`; the Linux wheel
  has no SVML and runs the scalar renderer with `powf`.
- **Every caller of an import:** `wheel-inspect imports --grep '^log2f?$'` lists `log2` and
  `log2f` with their call sites per function in both builds. The Linux names carry the glibc
  symbol version: the wheel links `log2@GLIBC_2.2.5`, `log@GLIBC_2.2.5` and
  `pow@GLIBC_2.2.5` (the old compatibility wrappers) but `log2f@GLIBC_2.27`.
- **SVML** is linked statically into the DLL, so no import names it. `known.py` names the two
  routines 2.5.2 contains: `__vdecl_powf4` (`0x18034b0b0`, a `jmp` to the kernel) and
  `__sse2_powf4` (`0x18034ba20`, the only function that reads the `_RDATA` section, where the
  CRT keeps SVML's constants). They were identified by linking `_mm_pow_ps` with MSVC 14.44
  (`cl /O2 /MD t.c /link /MAP`): the map file names `__vdecl_powf4` and `__sse2_powf4@@32`, and
  the kernel's 594 instructions and constants equal the DLL's. Unnamed code that reads
  `_RDATA` is probably another CRT math routine; identify it the same way.
- **FMA and F16C anywhere:**
  ```
  $ wheel-inspect scan 'vfn?m(add|sub)(132|213|231)(ps|ss|pd|sd)' --linux
    108 instructions match ..., in 14 functions
         6  0x4c4fd0  void OpenColorIO_v2_5::(anonymous namespace)::linear1D<...>(...)
        ...
        18  0x4dab60  void OpenColorIO_v2_5::(anonymous namespace)::applyTetrahedralAVX2Func<...>(...)
        18  0x4db260  void OpenColorIO_v2_5::(anonymous namespace)::applyTetrahedralAVX512Func<...>(...)
  ```
  Each library has 108 FMA instructions, all in the Lut1D kernels (`linear1D`, 72) and the Lut3D
  AVX2/AVX-512 tetrahedral kernels (36), none in double precision; and 24 F16C instructions
  (`scan 'vcvt(ph2ps|ps2ph)'`), all `vcvtps2ph` in the Lut1D kernels' half stores. MSVC used
  only `vfmadd231ps`; GCC used `vfmadd132ps` (99) and `vfmadd231ps` (9). For one function:
  `disasm NAME | grep -c vfmadd`.

## Where does the Windows build compute in float and Linux in double?

Example: `LogUtil::GetLogSideBreak` (`LogUtils.cpp:270-281`) calls `log2` on `float`s.

1. **Compare the calls:** `wheel-inspect imports --grep '^log2f?$'` shows Linux calling
   `log2@GLIBC_2.2.5` (double) from `GetLogSideBreak` where Windows calls `log2f`. A function
   that calls the double function on one platform and the float one on the other is a
   candidate.
2. **Compare the arithmetic:** `wheel-inspect disasm LogUtil::GetLogSideBreak`. Suffixes tell
   the type: `ss` is float, `sd` double; `cvtss2sd`/`cvtsd2ss` convert.
   ```
   linux (0x4017b0), excerpt                 windows (0x18021ac60), excerpt
   cvtss2sd xmm0, xmm0                       cvtpd2ps xmm0, xmm0
   call     log2@GLIBC_2.2.5 (x2)            call     log2f (x2)
   divsd    xmm1, xmm0                       divss    xmm6, xmm0
   mulsd    xmm1, xmm0      ; q * lsb        mulss    xmm7, xmm6    ; lsb * q
   cvtsd2ss xmm1, xmm1                       addss    xmm7, xmm1    ; lsb + logOffset
   addss    xmm0, xmm1      ; logOffset + lsb
   ```
   Linux divides and multiplies in double and adds in float; Windows does everything in float.
   The cause is C++ overload resolution: with MSVC `<cmath>` puts `log2(float)` in the global
   namespace, with libstdc++ this file only sees the C library's `double log2(double)`
   (s2-s5.md, "Windows and Linux differences" 1). The port has both variants, chosen with
   `cfg(target_os)`.
3. **The operand orders differ too** (Linux `q * lsb`, `logOffset + lsb`; Windows the source
   order), so the port pins each platform's order where two NaNs can meet.

## Where the names come from

- **Linux:** the `.so`'s full symbol table, demangled by the Rust `cpp_demangle` crate. Its
  spelling differs from `c++filt` in places (`std::ostream` for
  `std::basic_ostream<char, std::char_traits<char> >`, `(unsigned long)3` for `3ul`); 155 of
  19,783 symbols stay mangled. Imports show their library and symbol version.
- **Windows:** the DLL has no symbols.
  - Function bounds come from `.pdata`, with chained entries (code MSVC moved out of line)
    merged; code without unwind data (leaves, thunks) is bounded by the `int3` padding.
  - **Virtual functions** are named through RTTI: type descriptor, complete object locator,
    vtable. Slot *k* of a class's vtable gets the Linux symbol in the matching slot of the same
    class's vtable (GCC's two destructor entries are one MSVC slot; overloaded virtuals, which
    MSVC reorders, stay unnamed). A template instantiation only MSVC compiled (the SVML PQ
    renderers) borrows its method names from another instantiation of the same template.
  - **Exports** are named from their decorated names (without parameters).
  - `tools/wheel-inspect/src/wheel_inspect/known.py` names the rest, each entry with how it was
    identified.
  - Anything else is `sub_<address>`. For such a function, `find` lists the Windows functions
    that call the same imports as the Linux function, with `f` suffixes folded (`log2f` and
    `log2` count as one). That is how `GetLogSideBreak` was found: it is the only function
    that calls `log2f` twice. Confirm a candidate by its arithmetic before relying on it, then
    add it to `known.py` (address, full Linux name, kind, evidence); the selftest checks every
    entry.

## Why capstone

The disassembler is capstone 5.0.9 from PyPI, pinned in `tools/wheel-inspect/uv.lock`, so its
text is the same on every machine; the spikes used GNU objdump (binutils 2.44 from MSYS2 on
Windows, 2.35.2 in Rocky), whose versions differ between the two platforms. Over every
executable section of both libraries, capstone and objdump agree on each instruction's
boundaries and mnemonic wherever they decode code. They differ only in data embedded in `.text`
(MSVC's switch tables, which both decode as garbage and name differently) and in how prefixes
are printed (`rex`, `lock`, multi-byte `nop`). Both count 108 FMA and 24 F16C instructions per
library. capstone does no symbolization, so the tool does its own from the symbol table,
`.pdata`, RTTI, the import tables and `known.py`.

## selftest

`wheel-inspect selftest` (and `tools/wheel-inspect/rocky.sh selftest`) checks, on both
libraries:

- the wheels' hashes, and that the oracle environment's copy is the same file;
- the operand order at every site of s2-s5.md's table, and that `find` names the function
  holding it in both builds;
- `GetLogSideBreak`'s float and double arithmetic and calls on each platform, and that the
  import fingerprint finds it on Windows;
- 108 FMA and 24 F16C instructions per library, and on Linux which kernels hold them;
- `__vdecl_powf4`'s 20 call sites in the PQ renderers, the Linux PQ renderers' `powf`, and the
  glibc symbol versions;
- every `known.py` entry.

It ends with `43 passed, 0 failed` on both platforms and exits non-zero on any failure.
