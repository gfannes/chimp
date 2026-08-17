# Chimp Specification &&:chimp:docs:spec

Chimp builds a Forest from one or more Groves. A Grove is a folder tree scanned
for Markdown and source files.

## Groves &&groves

- Global CLI flags must be specified before the command.
- `chimp lsp [-C PATH...]` runs a stdio LSP server. Stdout contains only framed
  JSON-RPC messages; optional logging is written to stderr.
- `-V LEVEL`/`--verbose LEVEL` accepts levels 0 through 4 and defaults to 1:
  0 emits required command output only and suppresses diagnostics; 1 reports
  enough context to identify a failure; 2 may warn about suspicious input
  without significant extra work; 3 reports every processed folder and file;
  and 4 may emit detailed parsing and aggregation activity.
- `-c FILE`/`--config FILE` selects the config file.
- Human-readable output is valid Markdown. An enumeration follows its heading
  or introduction directly, without an intervening blank line.
- `chimp chores -o FILE` and `chimp wbs -o FILE` write results to `.naft`,
  `.md`, or `.markdown` files based on the destination extension.
- `chimp config` displays the effective merged configuration, including the
  default assignee and resolved Grove paths and scan settings.
- Invalid configuration is rejected with the config filename and source line.
- `chimp config` is currently read-only; configuration mutation can be added
  later through subcommands.
- Grove config is read from `~/.config/chimp/config.toml` and local `chimp.toml`.
- A Grove can specify `path`, optional `extensions`, and optional `max_filesize`.
- Config can specify top-level `default_assignee = "name"` for unassigned
  Chores.
- Config can specify top-level `editor`. Editor selection falls back to the
  `EDITOR` environment variable and then `nvim`.
- `extensions` is per Grove; omitted extensions use the built-in
  Markdown/source extension set.
- `includes` is accepted as an alias for `extensions`.
- `max_filesize` skips files over the threshold before reading or parsing.

## Checks &&checks

- `chimp check` scans effective Groves and prints metadata/parsing diagnostics.
- Check diagnostics include unresolved AmpPaths, ambiguous AmpPath references,
  relative Definitions without a higher-level Definition, WBS metadata without a
  same-line Definition, unresolved or ambiguous assignees, duplicate Definition
  declarations, and Markdown parsing issues.

## Language server

- The server synchronizes open documents incrementally using UTF-16 LSP
  positions and rebuilds the Forest from disk plus unsaved document overlays.
- It supports completion, Definition and reference navigation, document and
  workspace symbols, and reload code actions/commands.
- Prototype-compatible Chore navigation maps declaration to active Chores in
  the current file, implementation to all active Chores, and type-definition
  to the first Chore in each sorted file segment.
- `[lsp] max_array_size` is a positive integer, defaults to 100, and caps
  workspace-symbol results.

## Metadata &&metadata

- [ ] TODO &metadata:grammar &@geert Define the complete permissive v1 metadata scanner.
- AmpPaths start with `&` and normally end at whitespace.
- Empty AmpPaths such as `&`, `&&`, or `&:` are omitted from the model and
  reported by `chimp check` as `EmptyAmpPath` issues.
- Date metadata uses `&YYYYMMDD`. An optional `+Nm` suffix adds calendar
  months, for example `&20260806+1m` becomes `20260906`; days beyond the end of
  the destination month are clamped to its last day.
- Backticks quote one AmpPath part. Spaces and colons inside that part are
  literal rather than token or path separators; for example,
  ``&project:`release 1: beta`:task`` has the three parts `project`,
  `release 1: beta`, and `task`. Backticks cannot be escaped or used as data.
- Wikilink references such as `&[[a/b]]` are normalized to colon-separated
  AmpPaths and behave like `&a:b`.
- Every AmpPath part is case-insensitive. Normalized Definition paths and
  assignee names are stored lowercase, including text inside quoted parts.
- Definition AmpPaths start with `&&`.
- Absolute Definition paths start with a colon, for example `&&:chimp:parser`.
- Relative Definition paths extend an inherited higher-level Definition.
- Declaring the same resolved Definition more than once is ambiguous. Exactly
  one declaration may use the exclusive marker, such as `&&^:work`, to select
  the prime Definition while allowing `&&work` declarations in other Groves.
- `&&@name` creates Definition `name` and registers it as an assignee.
  An explicit Chore assignment `&@name` must match an assignee Definition;
  multiple matches require exactly one exclusive prime declaration.
- A bare `@name` token is an assignment only when `name` matches an assignee
  Definition. Bare mentions never produce assignee-resolution diagnostics from
  `chimp check`.
- Valid `YYYYMMDD` and `YYYY-MM-DD` substrings in folder and file names supply
  inherited date metadata to their leaf Chores and Definitions. The earliest
  date is used when several path components contain dates.
- A filename stem beginning with a lowercase letter and containing a later
  uppercase letter creates a synthetic file-level TODO Chore. A stem beginning
  uppercase creates the corresponding DONE Chore. The synthetic Chore inherits
  the file's resolved Definitions and path-derived date, and is rendered as a
  Markdown checkbox item using the stem without its extension. This applies
  only when a trailing-`&` Definition has created the file's automatic leaf
  Definition; stopping that cascade also disables the filename Chore.
- A Definition ending in `&`, such as `&&:knowledge&`, enables filesystem
  Definition derivation below its location.
- For each scanned file under a cascading Definition, nested folder names and
  the filename without its final extension are appended as Definition parts.
  Generated parts are lowercase and backtick-quoted when punctuation or spaces
  require it.
- A nearer folder `&.md` containing a non-trailing Definition stops an inherited
  filesystem cascade for that subtree. A new trailing Definition starts a new
  cascade. A first-line file Definition applies the same stop/restart rule to
  that file.
- Chore status tags are `TODO`, `GO`, `WIP`, `DONE`, `QUESTION`, `INFO`,
  `BLOCKED`, `FORWARD`, `PLANNED`, `CANCELED`, and `ASSIGNED`.
- Markdown task checkboxes map `[ ]`, `[*]`, `[/]`, `[x]`, `[?]`, `[i]`, `[!]`,
  `[>]`, `[<]`, `[-]`, and `[~]` to those statuses in order.
- `&20260805` is a date, `&#12` is order, and `&@geert` is assignee metadata.
- `&^@geert` is an exclusive assignee. It discards assignees inherited from
  broader scopes; narrower scopes can then add assignees to `geert`.
- `&^#12` is an exclusive order. If multiple related exclusive orders disagree,
  Chimp reports a diagnostic and uses the lowest exclusive order for display.
- `&?project` marks the same-line Definition as a WBS type such as portfolio,
  program, project, epic, story, task, feature, requirement, or subtask.
- `chimp wbs` reports only Chores with WBS metadata on the Chore or a connected
  resolved Definition.

## Chores &&chores

- A Chore is any parsed line that contains an AmpPath, a status tag, or a
  CommonMark task checkbox.
- A Chore inherits AmpPaths from Markdown ancestors, file first-line metadata,
  and `&.md` folder metadata files from its folder toward the Grove root.
- `chimp chores` searches loaded Groves from config by default.
- Repeated `-C PATH` options add Grove roots to a search.
- Positional `chores` arguments are query terms; one or more `@name` terms filter
  Chores assigned to any listed assignee.
- `text:TERM` filters case-insensitively against raw Chore-line text only. Shell
  quoting can keep spaces in a phrase, as in `'text:release blocker'`.
- Query terms are combined with AND. Unprefixed terms retain the broad search
  across Chore text, file paths, Definition paths, and Definition assignees.
- `chimp chores --help` describes the query syntax and how ordinary and
  assignee terms combine.
- Chores without a direct or related Definition assignee match the configured
  `default_assignee`, if present.
- `chimp chores` reports only TODO, GO, WIP, QUESTION, and BLOCKED Chores.
- A Chore is visible only when the earliest of its direct date and all related
  Definition dates is today or earlier. A Chore without any such date remains
  visible.
- `-n COUNT` limits reporting to the first COUNT Chores after filtering and
  sorting.
- `-d`/`--details` appends line, order, and tag metadata after each Chore line
  and prints order section labels.
- Chore sorting uses a computed order from connected resolved Definitions.
- Chores are globally ordered across files. Chores without order are reported
  first; ordered Chores follow from high order to small order.
- Chore output prints a file header when the globally ordered stream moves to a
  different file.
- A reference with a trailing ampersand is an inverse injection target. For
  `&a &b &target&`, `target` is omitted from the current Chore and Definitions
  `a` and `b` are related to Definition `target` instead.
- Every trailing-ampersand target on a line receives all ordinary explicit
  AmpPaths from that line. Injected Definition relationships are transitive and
  cycle-safe when expanded onto Chores.
- Wikilinks accept the same inverse marker: `&[[target/path]]&` behaves like
  `&target:path&`.

## Source comments &&source-comments

- In supported programming-language files, a comment is scanned only when its
  content starts with an AmpPath.
- After that leading AmpPath selects the comment, the entire comment content is
  scanned for additional AmpPaths, statuses, and metadata.
- The comment may begin at any source column.
- A Chore related to `&:a:b:c` is also related to existing ancestor Definitions
  `a:b` and `a`.

## Debug &&debug

- `chimp debug` prints files, Definitions, Chores, computed metadata, and
  diagnostics in a human-readable format.

## Editor integration &&editor

- `chimp chores`, `chimp wbs`, `chimp check`, and `chimp debug` accept
  `-e`/`--edit`.
- Editing opens the first reported item for each distinct file at its line and
  column after producing the normal report.
- `-n COUNT` limits the number of files opened. For `chores`, it also limits the
  report as documented above.
- Configured editor commands can contain arguments with `{file}`, `{line}`, and
  `{column}` placeholders. Without placeholders, known editors receive their
  native location syntax and other editors receive `FILE:LINE:COLUMN`.

## Naft &&naft

- NAFT stands for Nodes with Attributes, Free text and Tags.
- `chimp naft encode OUT.naft FOLDER...` writes a readable single-file
  representation of one or more folder trees.
- Encode skips hidden paths and `.gitignore` matches by default. `-u` includes
  hidden paths; `-U` includes ignored paths.
- Encode errors include the path that failed at default verbosity level 1.
- At verbosity level 3, encode prints each processed folder and file.
- `chimp naft decode IN.naft BASE_FOLDER` decodes that representation below a
  base folder and fails if a target path already exists.
- Filesystem nodes use `[Folder](name:...)` and
  `[File](name:...)(content:...)`, with nested children in `{}`.
- Non-UTF8 file content is represented as `[File](name:...)(content_base64:...)`.
- Escaping is context-aware. Attribute values may contain literal `[]`, `{}`,
  `:`, and balanced `(...)`; only `\` and ambiguous unbalanced parentheses need
  escaping. Tags may contain balanced `[...]`; only `\` and ambiguous
  unbalanced brackets need escaping. Attribute keys remain conservative and
  escape `\`, `:`, `(`, and `)`.
- Whitespace between nodes, attributes, and child blocks is insignificant.

## Storage &&storage

- [ ] TODO &storage:roundtrip Keep original file bytes separate from parsed spans.
- Loading and writing without edits must produce byte-identical files.
- Export destinations must not be inside any Grove path.
- Export can include Amp metadata as loaded or strip `&...` metadata tokens.
- Export can filter files by Chore status, matching Amp tags, and file extension.

## Resolution &&resolution

- Non-Definition AmpPaths resolve to Definitions by exact path first.
- Resolution next accepts a unique contiguous suffix or final-part match.
- If those matches fail, a multi-part reference is matched backward as an
  ordered subsequence of Definition parts. The final parts must match, but
  intermediate Definition parts may be omitted from the reference. Reference
  parts cannot be reordered or omitted.
- Partial resolution succeeds only for one candidate. Multiple candidates
  produce an `AmbiguousAmpPath` check issue; no candidate produces an
  `UnresolvedAmpPath` issue.
- If no unique Definition exists, Chimp creates a phony Definition.
