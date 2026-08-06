# Chimp

Chimp scans Markdown and source comments for lightweight chore metadata.

AmpPath references can use colon notation such as `&a:b` or wikilink-compatible
notation such as `&[[a/b]]`; both resolve to the same Definition path.
Backticks quote a path part containing spaces or literal colons, for example
``&project:`release 1: beta`:task``.

See [the data model](docs/data_model.md) for the relationships between Groves,
source files, Definitions, Chores, computed metadata, and diagnostics.
See [the technology stack](docs/tech_stack.md) for current implementation
choices and potential future alternatives.

```sh
cargo run -- scan .
cargo run -- config
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
Source-code comments are scanned only when their content begins with an
AmpPath; once selected, the complete comment is scanned for more metadata.
AmpPath matching is case-insensitive and canonical paths are stored lowercase.
If exact and contiguous suffix resolution fail, Chimp can omit intermediate
Definition parts while matching backward. For example, `&company:api:release`
can resolve `company:platform:api:release` when that partial match is unique.

Global flags go before the command. `--nocolor` disables ANSI color output.
`-V LEVEL` or `--verbose LEVEL` sets verbosity; the default is level 1. Level 0
suppresses all optional diagnostics, including errors. Level 1 reports enough
context to identify failures. Level 2 can report suspicious input without an
expensive analysis pass. Level 3 reports every folder and file processed, and
level 4 may report detailed parsing and aggregation activity.

Human-readable command output is Markdown. Reports put enumerations directly
after their heading or introductory line, without an extra blank line.

`export` writes loaded files to a destination outside all Grove roots. By
default, exported files include Amp metadata exactly as loaded. Use `--strip-amp`
to omit `&...` metadata tokens from exported text. Use `--status`, repeated
`--amp`, and repeated `--ext` filters to limit what is exported.

`chores` loads Groves from `~/.config/chimp/config.toml` and `chimp.toml` by
default. Use repeated `-C PATH` options to add Groves. Positional arguments are
search terms; terms like `@geert` filter by assignee. A `text:TERM` argument
matches only raw Chore-line text, case-insensitively. Quote the complete shell
argument for phrases, for example `chimp chores 'text:release blocker'`.
Multiple query terms are combined with AND. Output uses ANSI color by default;
put `--nocolor` before the command to disable it. Use `--details` to
append line and tag metadata after each Chore. Use `-n COUNT` to show only the
first COUNT Chores after filtering and sorting. Chores are sorted globally
across files; file headers are printed only when the output stream moves to a
different file. Config can specify `default_assignee`; unassigned Chores match
that assignee. Multiple assignee filters such as `@geert @alice` match either
assignee. An exclusive assignee such as `&^@geert` clears assignees inherited
from broader scopes; narrower scopes continue from `geert`. DONE Chores and
CANCELED Chores, including Markdown `[-]` items, are not reported by `chores`;
the visible statuses are TODO, GO, WIP, QUESTION, and BLOCKED.
Within source content, a bare mention such as `@geert` is also treated as an
assignment when `&&@geert` declares that name as an assignee. Unmatched bare
mentions remain ordinary text and do not produce `chimp check` issues.
Run `chimp chores --help` for the full query and option reference. Chores with
dates are shown only when their earliest direct or related Definition date is
today or earlier; undated Chores remain visible. A date can include a calendar
month offset, so `&20260806+1m` is stored as `20260906`. If the destination
month is shorter, the day is clamped to its final day.
Valid `YYYYMMDD` and `YYYY-MM-DD` dates embedded in folder or file names are
also inherited by their contained Chores and Definitions. When several path
components contain dates, the earliest date is retained.
Chimp also treats a filename stem as a file-level Chore when its casing carries
workflow state. A stem that starts lowercase and contains a later uppercase
letter, such as `draftProposal.md`, creates a TODO item. Capitalizing the first
letter (`DraftProposal.md`, with spaces also allowed) changes that item to DONE,
so it disappears from the normal `chores` report.

A trailing ampersand reverses an AmpPath relationship. On a line such as
`&urgent &release&`, `release` is not attached to that line's Chore; instead,
Definition `urgent` is injected into Definition `release`. A later Chore related
to `release` is consequently also related to `urgent`. Multiple trailing
targets inject the line's ordinary AmpPaths into every target, and injected
relationships are transitive.
Wikilink targets support the same inverse form, for example
`&urgent &[[release/desktop]]&`.
Empty AmpPaths such as a standalone `&` are ignored and reported by
`chimp check` rather than becoming phony Definitions.

A trailing ampersand on a Definition enables filesystem-derived Definitions.
For example, `&&:knowledge&` in a folder's `&.md` makes
`projects/My Notes.md` define ``knowledge:projects:`my notes``` and its folder
ancestor. A nearer `&.md` Definition without the trailing ampersand stops the
cascade for that subtree; a deeper trailing Definition can start a new one.
Definitions on the first line of an individual file similarly stop or restart
derivation for that file.

`config` prints the effective merged configuration, including the default
assignee and every Grove with its resolved path and scan settings. Configuration
syntax errors identify the config file and line. The command is intentionally
read-only for now; future subcommands can add, update, or delete Groves.

`chores`, `wbs`, `check`, and `debug` accept `-e`/`--edit`. After reporting,
Chimp opens the first reported item in each file at its line and column.
`-n COUNT` limits how many files are opened; for `chores`, it also retains its
existing meaning of limiting reported Chores. The editor is selected from the
configured `editor`, then `$EDITOR`, and finally `nvim`. Vim, Neovim, Emacs,
VS Code, and VSCodium receive their native location arguments. Other editors
receive `FILE:LINE:COLUMN`. Configured editor arguments may use `{file}`,
`{line}`, and `{column}` placeholders.

`check` loads the effective Groves and prints Amp metadata diagnostics such as
unresolved AmpPaths, ambiguous AmpPath references, relative Definitions without
an inherited parent Definition, WBS metadata without a same-line Definition,
and Markdown parsing issues. Repeated declarations of the same Definition are
ambiguous. Mark exactly one declaration as the prime with `^`, for example
`&&^:work`; other `&&work` declarations can then establish the same Definition
in other Groves without an ambiguity. `&&@alice` defines `alice` and registers
it as a valid assignee. Explicit `&@alice` Chore assignments must resolve to
exactly one such assignee Definition (or to one exclusive prime declaration).

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
editor = "nvim"

[[grove]]
path = "/home/geertf/project-docs"
extensions = ["md"]
max_filesize = 1048576

[[grove]]
path = "/home/geertf/project-src"
extensions = ["rs", "zig"]
```

When `extensions` is omitted, Chimp uses the built-in Markdown/source extension
set. `includes` is accepted as an alias for `extensions`. `max_filesize` is
optional and is measured in bytes.

WBS metadata uses `&?name`, for example `&?project`. It applies to a Definition
on the same line; `chimp check` reports `&?` markers that are not paired with a
same-line `&&...` Definition.

`wbs` shows only Chores that have WBS metadata on the Chore itself or on a
resolved Definition connected to it.
