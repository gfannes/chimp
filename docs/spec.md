# Chimp Specification &&:chimp:docs:spec

Chimp builds a Forest from one or more Groves. A Grove is a folder tree scanned
for Markdown and source files.

## Groves &&groves

- Global CLI flags must be specified before the command.
- `-V LEVEL`/`--verbose LEVEL` sets reporting verbosity and defaults to 1.
- `chimp groves` displays the Grove paths and scan settings that will be used.
- Grove config is read from `~/.config/chimp/config.toml` and local `chimp.toml`.
- A Grove can specify `path`, optional `extensions`, and optional `max_filesize`.
- Config can specify top-level `default_assignee = "name"` for unassigned
  Chores.
- `extensions` is per Grove; omitted extensions use the built-in
  Markdown/source extension set.
- `max_filesize` skips files over the threshold before reading or parsing.

## Checks &&checks

- `chimp check` scans effective Groves and prints metadata/parsing diagnostics.
- Check diagnostics include unresolved AmpPaths, ambiguous AmpPath references,
  relative Definitions without a higher-level Definition, WBS metadata without a
  same-line Definition, and Markdown parsing issues.

## Metadata &&metadata

- [ ] TODO &metadata:grammar &@geert Define the complete permissive v1 metadata scanner.
- AmpPaths start with `&` and end at whitespace.
- Definition AmpPaths start with `&&`.
- Absolute Definition paths start with a colon, for example `&&:chimp:parser`.
- Relative Definition paths extend an inherited higher-level Definition.
- Chore status tags are `TODO`, `GO`, `WIP`, `DONE`, `QUESTION`, `INFO`,
  `BLOCKED`, `FORWARD`, `PLANNED`, `CANCELED`, and `ASSIGNED`.
- Markdown task checkboxes map `[ ]`, `[*]`, `[/]`, `[x]`, `[?]`, `[i]`, `[!]`,
  `[>]`, `[<]`, `[-]`, and `[~]` to those statuses in order.
- `&20260805` is a date, `&#12` is order, and `&@geert` is assignee metadata.
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
- Chores without a direct or related Definition assignee match the configured
  `default_assignee`, if present.
- `chimp chores` reports only TODO, GO, WIP, QUESTION, and BLOCKED Chores.
- `-n COUNT` limits reporting to the first COUNT Chores after filtering and
  sorting.
- `-d`/`--details` appends line, order, and tag metadata after each Chore line
  and prints order section labels.
- Chore sorting uses a computed order from connected resolved Definitions.
- Chores are globally ordered across files. Chores without order are reported
  first; ordered Chores follow from high order to small order.
- Chore output prints a file header when the globally ordered stream moves to a
  different file.
- A Chore related to `&:a:b:c` is also related to existing ancestor Definitions
  `a:b` and `a`.

## Debug &&debug

- `chimp debug` prints files, Definitions, Chores, computed metadata, and
  diagnostics in a human-readable format.

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
- If exact resolution fails, a unique suffix match resolves the reference.
- If no unique Definition exists, Chimp creates a phony Definition.
