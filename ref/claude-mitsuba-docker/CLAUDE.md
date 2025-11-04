This file provides guidance to Claude Code.

# Overview
The Mitsuba project implements a differentiable Monte Carlo renderer and
consists of a C++ library (`libmitsuba`) and C++ plugins for scene objects
(BSDFs, materials, emitters, etc.). Everything has Python bindings and can be
used and extended within Python. The project is implemented on top of the
Dr.Jit just-in-time compiler (`ext/drjit` directory), which provides array
classes and types that are *traced*, i.e., operations involving them construct
a computation graph that is later fused into a GPU megakernel. The renderer can
be simultaneously compiled in multiple different _variants_ (e.g.,
`scalar_rgb`/`scalar_spectral` for RGB/spectral scalar CPU rendering,
`cuda_ad_rgb` for differentiable GPU ray tracing on CUDA, and `llvm_ad_rgb`
for LLVM-based JIT compilation to differentiable parallel CPU programs). The
project targets C++17 and Python 3.8+, uses CMake/Ninja for builds, and PyTest
for testing.

# Project structure
## Library
- The code is is partitioned into core and rendering-specific bits.
- Headers: `include/mitsuba/{core,render}/*.h`. The API is documented there.
- Implementation: `src/{core, render}/*.cpp`.
- Python bindings: `src/{core, render}/python/*.cpp`. Files ending with
  `_v.cpp` are specific to each variant and may be compiled multiple times.
- Tests: `src/{core,render}/tests/*.py`

## Plugins
Plugins are grouped in `src/$type/*`, where `$typhe` is one of `shapes`,
`emitters`, `bsdfs` (Bidirectional Scattering Distribution Functions),
`samplers` (Monte Carlo sample generators), `spectra`, `sensors`, `media`,
`phase` (phase functions), `textures`, and `rfilters` (reconstruction filters).
Each has a `tests` subdirectory with PyTest coverage.

# Building and debugging
- Build with `ninja -C build-claude`. If compilation fails, fix the issue and
  validate the fix by recompiling the failing target via `ninja -C build
  <path-to-file.cpp.o>` before returning to a full build. Never clean the build
  directory.
- Before using Dr.Jit/Mitsuba/running tests, call `source build-claude/setpath.sh`.
- Use `source build-claude/setpath.sh && python -m pytest -m 'not slow'
  to run the full test suite (takes ~10min), or `[..same prefix..] -m pytest
  path/to/test.py` for specific tests.
- When running small test snippets, they must set a variant first, e.g.,
  `import mitsuba as mi; mi.set_variant('cuda_ad_rgb')`.
- In the case of segfaults, run PyTest with `-v --capture no` to get more
  output. Use run `gdb --batch --quiet -ex "run" -ex "bt" -ex "quit" --args
  <COMMAND HERE>` to capture a C++ backtrace or `-e "py-bt"` for a
  Python backtrace. GDB can also break when an exception is raised (`-ex "b
  __cxa_throw"`) and report a backtrace.
- If tests fail with hardcoded paths hinting at the user's home directory (e.g.
  `/home/ziyizhang/..`), run `find . -name *.pyc -delete` to clear the bytecode cache.
- In the case of issues with Dr.JIT, its log level can be increased with
  `dr.set_log_level(x)` where `x` is 3 (summarize kernel launches), 4 (+ high
  level operations), 5 (+ list every IR operation), 6 (+ reference counting,
  generated IR).
- When tests fail, there might be warnings messages about leaks and/or
  unregistered pointers. Please ignore those.

# Code style
- **Naming**: Classes: PascalCase, Functions/methods/variables: snake_case,
  Member variables: prefixed with `m_`
- In the C++ codebase, lower case types (``uint32_t``) are scalar, while
  capital versions (``UInt32``) are Dr.Jit types that will be traced in
  non-scalar variants.
- **Formatting**: 4-space indentation, opening braces on same line. Avoid
  braces in `if` statements or `for/while` loops when the body is a short
  one-liner. In sequences of if/else statements, either put braces for all of
  them or none at all. When casting types, leave a space, e.g.: `(T) expr`.
- **Whitespace**: avoid whitespace errors (empty lines with trailing whitespace
  characters). Regularly run `git diff --check` to check for this.
- **Namespace**: Code wrapped in `NAMESPACE_BEGIN(mitsuba)` and
  `NAMESPACE_END(mitsuba)` macros
- **Includes**: Standard library first, then project includes (`<mitsuba/...>`)
- **Templates**: The codebase contains many so-called "variant" classes, which
  are templated by a numeric type (`Float`) and a spectrum type (`Spectrum`).
  The often used `MI_VARIANT` macro encodes the associated `template <typename
  Float, typename Spectrum>` declaration.
- **Python**: Add proper docstrings, clean imports. Update Python bindings to match
  C++ API changes.
- **Error handling**: Use `Throw` and `Assert` for exceptions/assertions in C++.
- **Memory**: `mitsuba::Object`-derived types use intrusive reference counting.
  They are stored using a `ref<>` helper that holds a reference.
- Avoid: `thread_local`, `std::unordered_map`, `std::map`, `std::set` (prefer
  `tsl::robin_map/set` for hash tables/sets). Prefer simple self-contained
  C/C++ code over complex use of STL containers.

# Coordination
- If asked to address comments, run `rg CLAUDE` to find them in the codebase.
  Ignore other keywords like TODO, FIXME, XXX, etc.
- NEVER run git commands that modify the repository or staging area, unless
  asked to do so. When asked to commit changes use `git add <file>` and `git
  commit -a -m "message"`. Never add all files (`git add -A`), which pulls in
  build byproducts.
