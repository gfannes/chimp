# Chimp data model

Chimp turns one or more filesystem trees into an in-memory `Forest`. The
`Forest` is the shared model used by checking, debugging, Chore queries, WBS
queries, and export.

```text
Config
└── GroveConfig (one or more)
    └── SourceFile (zero or more)
        ├── Chore (zero or more)
        │   └── DefinitionId ─────────┐
        └── source locations          │
                                      ▼
Forest ── definitions ─────────── Definition
       └─ issues ──────────────── CheckIssue
```

IDs are indexes into vectors owned by the same `Forest`. They are stable for
the lifetime of that `Forest`, but they are not persistent identifiers and must
not be compared across separate scans.

## Configuration and Groves

`Config` contains a list of `GroveConfig` values. A Grove is an independently
configured filesystem root.

| Field | Type | Meaning |
|---|---|---|
| `root` | path | Root folder scanned for source files. |
| `extensions` | list of strings | Allowed extensions. An empty list selects Chimp's built-in extension set. |
| `max_filesize` | optional integer | Maximum number of bytes read per file. |

The CLI also has `default_assignee`. This is part of the effective CLI
configuration rather than the core `Config`, because it affects Chore querying
and display but not Forest construction.

A `SourceFile.grove` value is the zero-based index of its owning `GroveConfig`.
Definitions may resolve across all Groves in a Forest; a Grove is a scan and
metadata-inheritance boundary, not a Definition namespace.

## Source files

Each scanned file becomes a `SourceFile`.

| Field | Type | Meaning |
|---|---|---|
| `id` | `FileId` | Index into `Forest.files`. |
| `grove` | integer | Index of the owning Grove. |
| `root` | path | Canonical Grove root. |
| `path` | path | Full source path. |
| `bytes` | byte array | Original bytes, retained for exact export and round trips. |
| `text` | string | Lossy UTF-8 view used by the metadata parser. |

Markdown files are parsed as document structure. Supported source-code files
are parsed only where a line is recognized as a source comment whose content
begins with an AmpPath; the rest of a selected comment is then scanned. Markdown code
spans, fenced code blocks, inline formulas, and formula blocks are excluded
from metadata extraction.

An `&.md` file is special: its metadata supplies folder context to other files
below that folder in the same Grove.

A trailing-ampersand Definition in folder metadata also creates a filesystem
context. Chimp appends the relative folder components and file stem to that
Definition and attaches the generated leaf Definition to every Chore in the
file. Existing generated ancestors are related through the usual ancestor
expansion. A nearer non-trailing folder Definition clears the active context;
a later trailing Definition replaces it with a new base.

## Parsed metadata

`Metadata` is a transient line-level value used while constructing a Forest.
It is not stored as a single object in the finished model; its fields are
distributed over Definitions and Chores.

| Field | Source notation | Meaning |
|---|---|---|
| `definitions` | `&&...` | Definition declarations. |
| `references` | `&...` | AmpPath references. |
| `status` | status word or checkbox | Chore workflow status. |
| `checkbox` | Markdown checkbox | Whether the legacy Boolean checkbox view is completed. |
| `date` | `&YYYYMMDD` or `&YYYYMMDD+Nm` | Valid calendar date, normalized to `YYYYMMDD` after applying an optional month offset. |
| `order` | `&#N` or `&^#N` | Numeric order and optional exclusivity. |
| `assignee` | `&@name` or `&^@name` | Explicit Chore or Definition assignee. |
| `assignee_exclusive` | `^` in assignee metadata | Whether this assignee clears broader inherited assignments. |
| `bare_assignees` | `@name` without `&` | Candidate assignments, accepted only when an assignee Definition exists. |
| `empty_amp_paths` | empty `&...` token | Count used to emit check issues; no relationship is retained. |
| `wbs` | `&?name` | WBS classifications. |

Definition and reference context is collected from these scopes:

1. `&.md` folder metadata, from the Grove root toward the file's folder.
2. Metadata on the first line of the file.
3. Enclosing Markdown headings and list items.
4. Definitions and references written directly on the current line.

Only Definition and reference AmpPaths are inherited by a Chore. Scalar Chore
fields such as status, date, order, and assignee come from the Chore's own line;
metadata on inherited Definitions remains available through the Chore's
`definitions` links. Where `Metadata` values themselves are merged, lists are
accumulated and the first available scalar value wins.

## Definitions

A Definition gives a normalized AmpPath a location and metadata.

| Field | Type | Meaning |
|---|---|---|
| `id` | `DefinitionId` | Index into `Forest.definitions`. |
| `path` | string | Lowercase normalized colon-separated AmpPath without leading ampersands or colons. |
| `is_phony` | Boolean | The Definition was synthesized because a reference could not resolve uniquely. |
| `exclusive` | Boolean | This declaration uses `^` and is the prime among repeated declarations. |
| `is_assignee` | Boolean | The Definition was declared with `&&@name` and can validate assignments. |
| `file` | optional `FileId` | Source file of the selected declaration. |
| `line` | optional integer | One-based source line of the selected declaration. |
| `date` | optional string | Definition date metadata. |
| `order` | optional `OrderMetadata` | Definition order metadata. |
| `assignee` | optional string | Assignee metadata attached to the Definition. |
| `assignee_exclusive` | Boolean | Whether the Definition assignee clears assignments inherited from broader Definition paths. |
| `wbs` | list of strings | WBS classifications attached to the Definition. |
| `definitions` | list of `DefinitionId` | Definitions injected into this Definition by trailing-ampersand references. |

### Paths and resolution

An absolute declaration such as `&&:chimp:parser` becomes
`chimp:parser`. A relative declaration extends the nearest inherited Definition.
For example, `&&parser` under `&&:chimp` also becomes `chimp:parser`.

Backticks preserve spaces and colons inside a single path part. The canonical
path retains those backticks so structural separators remain distinguishable:
``&project:`release 1: beta`:task`` has depth two, while the colon between
`1` and `beta` is data. Backticks are delimiters only; escaping or literal
backticks inside an AmpPath are not supported.

A reference is resolved in this order:

1. Exact normalized path.
2. A unique Definition whose path has the reference as a suffix or final path
   component.
3. A unique partial Definition match. Matching proceeds backward from an equal
   final part; reference parts must occur in order, while intermediate
   Definition parts may be skipped. For example, `company:api:release` matches
   `company:platform:api:release`.
4. A phony Definition when there is no unique match.

Wikilink reference syntax is normalized before resolution: `&[[a/b]]` becomes
the same path as `&a:b`. The original wikilink text remains in the Chore's
source text.

The final case also produces either an `UnresolvedAmpPath` or
`AmbiguousAmpPath` issue. A Chore related to `a:b:c` is additionally related to
existing ancestor Definitions `a:b` and `a`.

An AmpPath without a path part is discarded during extraction. Each occurrence
produces an `EmptyAmpPath` check issue at its source file and line; it never
creates a phony Definition or a Chore relationship.

### Repeated and exclusive declarations

All declarations with the same resolved path describe one logical Definition.
Multiple declarations are ambiguous unless exactly one carries the exclusive
marker, for example `&&^:work`. The exclusive declaration supplies the prime
location and scalar metadata used by ordinary commands. WBS values from
declarations are deduplicated and accumulated.

Declaration occurrences are retained only while building and validating the
Forest. The finished `Forest.definitions` collection contains one Definition
per normalized path.

### Inverse injection

A reference ending in `&` reverses the usual Chore relationship. Given
`&urgent &release&`, `release` is not related to the current Chore. Instead,
Definition `urgent` is stored in `release.definitions`. If a line contains more
than one trailing-ampersand reference, every target receives the line's other
explicit Definition and reference AmpPaths.
Wikilink targets use the same rule, so `&[[release/desktop]]&` injects into
Definition `release:desktop`.

When a Chore later resolves `release`, its Definition list is expanded with
these injected relationships. Expansion is transitive and uses the existing ID
set to terminate cycles. Existing ancestors of injected Definitions are added
after expansion. Metadata such as order, assignee, and WBS therefore continues
to use the normal related-Definition aggregation rules.

### Assignee Definitions

`&&@alice` creates the normalized Definition `alice` and marks it as an
assignee Definition. Every explicit `&@alice` assignment must match at least
one such declaration. Repeated assignee declarations require one exclusive
prime, just like other Definitions.

## Chores

A parsed line becomes a `Chore` when it contains a Definition or reference, a
status, a supported checkbox, or WBS metadata.
Additionally, filename casing can create a synthetic file-level Chore. A stem
that begins lowercase and has an uppercase letter later is TODO; a stem that
begins uppercase is DONE. These Chores use source position 1:1, Markdown task
text derived from the extensionless stem, and the file's Definition and date
context. Stems matching neither pattern create no file-level Chore.

| Field | Type | Meaning |
|---|---|---|
| `file` | `FileId` | Owning source file. |
| `line` | integer | One-based source line. |
| `column` | integer | One-based content column. |
| `text` | string | Original Markdown line or extracted source-comment content. |
| `status` | optional `Status` | Workflow status. |
| `date` | optional string | Direct date metadata. |
| `order` | optional `OrderMetadata` | Direct order metadata. |
| `assignee` | optional string | Direct assignment. |
| `assignee_exclusive` | Boolean | Whether the direct assignment clears all inherited assignments. |
| `wbs` | list of strings | Direct WBS classifications. |
| `definitions` | list of `DefinitionId` | Resolved, inherited, and ancestor Definitions related to the Chore. |

The supported status values are `Todo`, `Go`, `Wip`, `Done`, `Question`,
`Info`, `Blocked`, `Forward`, `Planned`, `Canceled`, and `Assigned`. The model
retains every status; `chimp chores` applies a presentation filter and normally
shows only Todo, Go, WIP, Question, and Blocked items.

The effective assignees used by `chimp chores` are aggregated from related
Definitions in broad-to-narrow path order, followed by the direct Chore
assignee. A non-exclusive assignment is added to the current set. An exclusive
assignment such as `&^@geert` clears the current set before adding `geert`, so
it breaks inheritance while allowing narrower scopes to add new assignees. The
configured default assignee is used only when aggregation produces no assignee.
Bare `@name` candidates are resolved after explicit-assignee validation, so
ordinary mentions that do not resolve never create `chimp check` issues.

Chore reporting compares the earliest date among the Chore's direct `date` and
the dates of every related Definition with the current date. Future Chores are
hidden; undated Chores are visible. Month offsets use calendar arithmetic and
clamp the day to the destination month's last valid day.
Dates embedded in relative folder and file-name components (`YYYYMMDD` or
`YYYY-MM-DD`) are inherited by leaf Definitions and Chores. If multiple path
dates apply, the model retains the earliest normalized date.

## Order metadata

`OrderMetadata` contains a non-negative `u32` value and an `exclusive` flag.
The computed order for a Chore is derived from its related Definitions:

- If any related orders are exclusive, the lowest exclusive value is used.
- Conflicting exclusive values set `ComputedOrder.conflict` and produce a
  `ConflictingExclusiveOrder` issue.
- Otherwise, the lowest related order is used.
- With no related Definition order, the computed order is absent.

`ComputedOrder` is calculated on demand and is not stored in the Forest.
`Chore.order` retains direct line metadata, while the current computed-order
algorithm intentionally uses Definition orders.

## Diagnostics

`Forest.issues` contains non-fatal problems found during parsing, resolution,
and aggregation. A `CheckIssue` has a kind, optional file and line, and a
human-readable message.

The issue kinds are:

- `UnresolvedAmpPath`
- `AmbiguousAmpPath`
- `AmbiguousDefinition`
- `UnresolvedAssignee`
- `AmbiguousAssignee`
- `RelativeDefinitionWithoutParent`
- `WbsWithoutDefinition`
- `MarkdownParsing`
- `ConflictingExclusiveOrder`

Issues do not prevent Forest construction. `chimp check` reports them, while
other commands may continue with the selected or synthesized model objects.

## Export model

`ExportOptions` controls a projection of a Forest back to files. It can retain
or strip Amp metadata and filter files by Chore status, Amp tag, or extension.
`ExportSummary` currently reports only the number of files written. Export does
not mutate the Forest and refuses destinations inside a scanned Grove.

NAFT nodes are a separate serialization model for filesystem fixtures and
folder round trips. They are not part of the Forest data model.
