# Chimp technology stack

This document describes the stack currently present in the repository and
possible alternatives for future stages of Chimp. Alternatives are options,
not committed roadmap decisions; each should be introduced only when its
benefit outweighs the additional dependency and maintenance cost.

## Current stack

### Language and build

| Area | Current choice |
|---|---|
| Language | Rust |
| Rust edition | 2024 |
| Build and package tool | Cargo |
| Package shape | One library crate and one `chimp` binary |
| License | MIT |
| External dependencies | None |

Chimp currently builds entirely on the Rust standard library. `Cargo.lock`
contains only the `chimp` package. This keeps builds small and reproducible,
reduces supply-chain exposure, and makes the implementation easy to audit. The
tradeoff is that Chimp maintains several parsers and platform abstractions that
established crates could provide more completely.

No minimum supported Rust version is declared beyond the compiler support
implied by Rust edition 2024.

### Application architecture

The package is divided into a small set of modules:

| File | Responsibility |
|---|---|
| `src/main.rs` | CLI parsing, configuration loading, command dispatch, Markdown/ANSI reporting, and process exit behavior. |
| `src/lib.rs` | Public data model, Forest construction, Definition resolution, validation, Chore aggregation, computed order, and export. |
| `src/scan.rs` | Grove traversal, extension and size filtering, and `.gitignore` handling. |
| `src/parse.rs` | Metadata extraction, Markdown visibility rules, source-comment recognition, and Amp metadata stripping. |
| `src/naft.rs` | NAFT syntax, filesystem encoding/decoding, escaping, and Base64 support. |

The application is synchronous and single-process. A scan builds an in-memory
Forest and commands query or export that Forest. There is no database, cache,
background service, async runtime, or network protocol. See
[the data model](data_model.md) for the objects produced by a scan.

### Command-line interface

CLI arguments are parsed directly from `std::env::args`. Commands and flags are
matched manually. Errors use boxed standard errors and are rendered by the
binary according to verbosity level.

Editor integration launches configured commands with `std::process::Command`;
it does not invoke a shell. This avoids shell interpolation while supporting
explicit file, line, and column placeholders.

Human-readable stdout is Markdown. ANSI escape sequences add color by default;
`--nocolor` disables them. Progress, warnings, and failures go to stderr.

This approach has no dependency overhead and gives exact control over output.
It also means help generation, shell completion, argument validation, and
subcommand evolution must be implemented manually.

### Configuration

Chimp reads a deliberately small TOML-shaped configuration subset from:

1. `~/.config/chimp/config.toml`
2. `chimp.toml` in the current directory

The local configuration is merged after the user configuration. Parsing and
validation are implemented in `src/main.rs`. The parser supports the fields
Chimp currently needs and reports file-and-line errors, but it is not a general
TOML implementation.

### Parsing and source support

Metadata parsing is a custom, line-oriented scanner. Chimp currently recognizes:

- Markdown document structure relevant to headings and list nesting
- Markdown checkboxes, inline code, fenced code, and formula regions
- Comment lines in C, C++, Ruby, Rust, and Zig sources
- AmpPath Definitions, references, status, date, order, assignee, and WBS
  notation

The parser is intentionally permissive and preserves source text. It does not
build a complete Markdown or programming-language syntax tree.

### Filesystem and ignore handling

Filesystem traversal uses `std::fs`. Paths are sorted before processing for
deterministic results. Chimp skips hidden paths and applies a custom subset of
`.gitignore` rules unless the relevant include flags are set.

Original file bytes are retained alongside a lossy UTF-8 view. This supports
exact file export while still allowing metadata scanning in files containing
invalid UTF-8.

### Data and serialization

The Forest is an in-memory Rust model backed by vectors, hash maps, and IDs that
index those vectors. It is reconstructed for each invocation and has no stable
on-disk representation.

NAFT—Nodes with Attributes, Free text and Tags—is Chimp's custom readable
serialization format for filesystem fixtures and folder round trips. Its
parser, serializer, escaping, and Base64 codec are implemented without external
dependencies.

### Testing and quality checks

The repository uses Rust's built-in test framework:

- Unit tests live beside library and binary code.
- CLI integration tests launch the compiled binary with `std::process::Command`.
- Tests construct isolated folders under the system temporary directory.
- `cargo fmt` supplies formatting.
- `cargo clippy --all-targets -- -D warnings` supplies static linting.

There is currently no benchmark suite, property-based testing, fuzzing setup,
snapshot testing, code-coverage configuration, or checked-in CI workflow.

## Potential future alternatives

### CLI framework: `clap`

`clap` could replace manual argument parsing when `chimp config` gains nested
add/update/delete subcommands or when command-specific flags become harder to
maintain. It would provide generated help, typed values, aliases, validation,
and shell completions.

The current parser remains reasonable while the command surface is small and
output compatibility needs tight control.

### Configuration: `serde` and `toml`

Serde-derived configuration loaded by the `toml` crate would support the full
TOML syntax, stronger typed validation, escaping, and clearer schema evolution.
It becomes attractive when configuration gains more nested objects, optional
policies, or write support through `chimp config` subcommands.

Before adopting it, decide whether Chimp must preserve comments and formatting
when editing configuration. If it must, a syntax-preserving TOML editor such as
`toml_edit` is a better fit for mutations than deserialize-and-reserialize.

### Filesystem traversal and ignore rules: `ignore`

The `ignore` crate could replace the custom walker and `.gitignore` subset. It
supports Git-compatible matching, nested ignore files, negation, global ignore
rules, hidden-file controls, and parallel walking. This is likely beneficial if
users expect Chimp to match Git behavior exactly or scan large repositories.

`walkdir` is a smaller alternative when robust recursive traversal is needed
but full Git ignore semantics are not.

### Markdown parsing: `pulldown-cmark` or `comrak`

A CommonMark parser could replace parts of the line-oriented Markdown state
machine. This would improve correctness for nested blocks, escaped delimiters,
HTML blocks, multiline constructs, and source ranges.

`pulldown-cmark` is event-oriented and relatively lightweight. `comrak` exposes
an editable syntax tree and CommonMark/GFM features. Either option would need a
careful mapping back to exact source locations and Chimp's metadata inheritance
rules.

### Source parsing: Tree-sitter

Tree-sitter grammars could identify comments and structural scopes accurately
across more programming languages. This would help if Chimp begins inheriting
metadata through modules, types, or functions, or offers editor navigation.

The cost is substantially greater binary size, grammar management, and build
complexity. Simple comment scanning is preferable while Chimp only needs
line-level metadata.

### Diagnostics: `thiserror`, `anyhow`, and `miette`

The current `Box<dyn Error>` approach is compact but loses structured error
types and rich source context.

- `thiserror` could define stable library error enums.
- `anyhow` could simplify application-level context and error propagation.
- `miette` could render filename, line, column, source snippets, and actionable
  labels for configuration, Markdown, AmpPath, and NAFT diagnostics.

Structured library errors should precede rich rendering so callers are not
forced to parse message strings.

### Logging and observability: `tracing` or `log`

The current verbosity checks and `eprintln!` calls are sufficient for a local
CLI. `tracing` would become useful for nested scan spans, timing, structured
fields, multiple output formats, or an eventual long-running service. The
lighter `log` facade is adequate if only leveled text messages are needed.

Any migration should preserve the documented verbosity behavior and keep
required command output separate from diagnostics.

### Parallel scanning: Rayon or the `ignore` parallel walker

Large independent Groves and files can be scanned in parallel. Rayon offers a
simple data-parallel model; the `ignore` crate includes parallel filesystem
walking. Deterministic ordering would still require sorting aggregated results,
and issue/ID allocation must not depend on thread scheduling.

Parallelism is worth considering only after benchmarks identify scanning or
parsing as a bottleneck.

### Persistent index: SQLite

SQLite could store file fingerprints, Definitions, Chores, and relationships so
unchanged files do not need to be reparsed on every invocation. It also enables
indexed cross-Grove queries and provides a migration path for richer reporting.

Persistence introduces cache invalidation, schema migrations, locking, and
recovery concerns. A content-addressed or modification-time cache is a smaller
intermediate step. SQLite is preferable to an opaque embedded key/value store
when the model remains relational and needs inspection or ad-hoc queries.

### File watching: `notify`

The `notify` crate could maintain an incremental Forest for an interactive
daemon, editor integration, or continuously refreshed query UI. It should be
paired with a reconciliation pass because filesystem event delivery varies by
platform and can be incomplete.

### Editor integration: LSP crates

An LSP implementation could provide Definition navigation, references,
diagnostics, completion for AmpPaths and assignees, and Chore code lenses.
`tower-lsp` or lower-level `lsp-types` building blocks are possible foundations.
This direction benefits from structured errors, stable source spans, and an
incremental index first.

### Testing tools

Several focused tools could improve confidence as the grammar grows:

| Need | Potential tool | Benefit |
|---|---|---|
| CLI assertions | `assert_cmd` and `predicates` | Clearer exit/stdout/stderr tests. |
| Temporary files | `tempfile` | Automatic cleanup and safer unique paths. |
| Output fixtures | `insta` | Reviewable snapshots for Markdown and debug output. |
| Generated cases | `proptest` | Invariants for AmpPath, NAFT, escaping, and configuration parsing. |
| Parser hardening | `cargo-fuzz` | Finds panics and pathological parser inputs. |
| Performance | Criterion | Tracks scan, parse, resolution, and serialization regressions. |

NAFT remains useful for end-to-end filesystem fixtures even if some of these
testing libraries are adopted.

### Distribution and automation

Future release infrastructure could add:

- A CI matrix for formatting, Clippy, tests, and supported platforms
- Prebuilt binaries for Linux, macOS, and Windows
- `cargo-dist` or an equivalent release pipeline
- Package-manager distribution where demand justifies it
- Dependency auditing and license checks after external crates are introduced

## Suggested adoption order

The following sequence minimizes architectural churn:

1. Adopt `toml_edit` when configuration mutation is implemented, or
   `serde`/`toml` sooner if full TOML compatibility becomes urgent.
2. Adopt `ignore` when Git-compatible traversal or scan performance becomes a
   recurring source of bugs.
3. Introduce structured error types and richer diagnostics before editor or
   service integrations.
4. Add property tests and fuzzing as the metadata and NAFT grammars expand.
5. Benchmark before adding parallelism or persistence.
6. Add an incremental index and file watching before implementing LSP features.

The central constraint is source fidelity: future libraries should preserve
Chimp's deterministic output, exact source locations, original bytes, and
ability to explain how a Chore relates to its Definitions.
