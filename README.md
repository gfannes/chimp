# Chimp

Chimp scans Markdown and source comments for lightweight chore metadata.

```sh
cargo run -- scan .
cargo run -- groves -C .
cargo run -- check -C .
cargo run -- wbs -C .
cargo run -- chores -C . storage
cargo run -- export /tmp/chimp-export --strip-amp --status TODO --amp storage:roundtrip --ext md .
```

The initial implementation supports Markdown, C/C++, Ruby, Rust, and Zig files.
It skips hidden paths and `.gitignore` entries, extracts Chores, resolves AmpPath
Definitions, and keeps original bytes for exact round-trip writes.

`export` writes loaded files to a destination outside all Grove roots. By
default, exported files include Amp metadata exactly as loaded. Use `--strip-amp`
to omit `&...` metadata tokens from exported text. Use `--status`, repeated
`--amp`, and repeated `--ext` filters to limit what is exported.

`chores` loads Groves from `~/.config/chimp/config.toml` and `chimp.toml` by
default. Use repeated `-C PATH` options to add Groves. Positional arguments are
search terms; terms like `@geert` filter by assignee.

`groves` prints the effective Grove list that will be scanned.

`check` loads the effective Groves and prints Amp metadata diagnostics such as
unresolved AmpPaths, ambiguous AmpPath references, relative Definitions without
an inherited parent Definition, WBS metadata without a same-line Definition,
and Markdown parsing issues.

Example config:

```toml
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
