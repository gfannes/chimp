# Chimp Specification &&:chimp:docs:spec

Chimp builds a Forest from one or more Groves. A Grove is a folder tree scanned
for Markdown and source files.

## Groves &&groves

- `chimp groves` displays the Grove paths and scan settings that will be used.
- Grove config is read from `~/.config/chimp/config.toml` and local `chimp.toml`.
- A Grove can specify `path`, optional `extensions`, and optional `max_filesize`.
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
- Chore status tags are `TODO`, `DONE`, and `WIP`.
- `&20260805` is a date, `&#12` is order, and `&@geert` is assignee metadata.
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
- Positional `chores` arguments are query terms; `@name` terms filter assignees.
- Chore output includes the maximum order found on connected resolved
  Definitions.

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
