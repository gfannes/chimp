# Chimp

Chimp scans Markdown and source comments for lightweight chore metadata.

```sh
cargo run -- scan .
cargo run -- groves -C .
cargo run -- check -C .
cargo run -- debug -C .
cargo run -- wbs -C .
cargo run -- --nocolor -V 1 chores --details -n 5 -C . storage
cargo run -- export /tmp/chimp-export --strip-amp --status TODO --amp storage:roundtrip --ext md .
cargo run -- -V 3 naft encode /tmp/grove.naft -u -U .
cargo run -- naft decode /tmp/grove.naft /tmp/chimp-naft
```

The initial implementation supports Markdown, C/C++, Ruby, Rust, and Zig files.
It skips hidden paths and `.gitignore` entries, extracts Chores, resolves AmpPath
Definitions, and keeps original bytes for exact round-trip writes.

Global flags go before the command. `--nocolor` disables ANSI color output.
`-V LEVEL` or `--verbose LEVEL` sets verbosity; the default is level 1.

`export` writes loaded files to a destination outside all Grove roots. By
default, exported files include Amp metadata exactly as loaded. Use `--strip-amp`
to omit `&...` metadata tokens from exported text. Use `--status`, repeated
`--amp`, and repeated `--ext` filters to limit what is exported.

`chores` loads Groves from `~/.config/chimp/config.toml` and `chimp.toml` by
default. Use repeated `-C PATH` options to add Groves. Positional arguments are
search terms; terms like `@geert` filter by assignee. Output uses ANSI color by
default; put `--nocolor` before the command to disable it. Use `--details` to
append line and tag metadata after each Chore. Use `-n COUNT` to show only the
first COUNT Chores after filtering and sorting. Chores are sorted globally
across files; file headers are printed only when the output stream moves to a
different file. Config can specify `default_assignee`; unassigned Chores match
that assignee. Multiple assignee filters such as `@geert @alice` match either
assignee. DONE Chores and CANCELED Chores, including Markdown `[-]` items, are
not reported by `chores`; the visible statuses are TODO, GO, WIP, QUESTION, and
BLOCKED.

`groves` prints the effective Grove list that will be scanned.

`check` loads the effective Groves and prints Amp metadata diagnostics such as
unresolved AmpPaths, ambiguous AmpPath references, relative Definitions without
an inherited parent Definition, WBS metadata without a same-line Definition,
and Markdown parsing issues.

`debug` prints a human-readable dump of loaded files, Definitions, Chores,
computed order, related Definitions, and diagnostics.

`naft` means Nodes with Attributes, Free text and Tags. It encodes folders into
a readable single-file filesystem format and decodes that format into a base
folder. Decode refuses to write over existing paths. At verbosity 1, encode
errors include the path being processed; at verbosity 3, encode reports each
processed folder and file to stderr. Attribute values keep common source text
readable: `[]`, `{}`, `:`, and balanced `(...)` are written without escaping.
Encode skips hidden and `.gitignore`d paths by default; `-u` includes hidden
paths, and `-U` includes ignored paths. Non-UTF8 file bytes are stored as
`content_base64`.

Example config:

```toml
default_assignee = "geert"

[[grove]]
path = "/home/geertf/project-docs"
extensions = ["md"]
max_filesize = 1048576

[[grove]]
path = "/home/geertf/project-src"
extensions = ["rs", "zig"]
```

When `extensions` is omitted, Chimp uses the built-in Markdown/source extension
set. `max_filesize` is optional and is measured in bytes.

WBS metadata uses `&?name`, for example `&?project`. It applies to a Definition
on the same line; `chimp check` reports `&?` markers that are not paired with a
same-line `&&...` Definition.

`wbs` shows only Chores that have WBS metadata on the Chore itself or on a
resolved Definition connected to it.
