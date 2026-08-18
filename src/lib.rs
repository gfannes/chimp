pub mod lsp;
pub mod naft;
mod parse;
mod scan;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub use parse::{Metadata, extract_metadata};
pub use scan::{load_files, load_files_with_reporter, write_file_exact};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub struct Config {
    pub groves: Vec<GroveConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroveConfig {
    pub root: PathBuf,
    pub extensions: Vec<String>,
    pub max_filesize: Option<u64>,
}

impl Config {
    pub fn from_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            groves: roots.into_iter().map(GroveConfig::from_root).collect(),
        }
    }

    pub fn from_groves(groves: Vec<GroveConfig>) -> Self {
        Self { groves }
    }
}

impl GroveConfig {
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            root,
            extensions: Vec::new(),
            max_filesize: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefinitionId(pub usize);

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub grove: usize,
    pub root: PathBuf,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub text: Arc<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Todo,
    Go,
    Done,
    Question,
    Info,
    Wip,
    Blocked,
    Forward,
    Planned,
    Canceled,
    Assigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderMetadata {
    pub value: u32,
    pub exclusive: bool,
}

#[derive(Debug, Clone)]
pub struct Definition {
    pub id: DefinitionId,
    pub path: String,
    pub is_phony: bool,
    pub exclusive: bool,
    pub is_assignee: bool,
    pub file: Option<FileId>,
    pub line: Option<usize>,
    pub date: Option<String>,
    pub order: Option<OrderMetadata>,
    pub assignee: Option<String>,
    pub assignee_exclusive: bool,
    pub wbs: Vec<String>,
    pub definitions: Vec<DefinitionId>,
}

#[derive(Debug, Clone)]
pub struct Chore {
    pub file: FileId,
    pub line: usize,
    pub column: usize,
    pub text: String,
    pub status: Option<Status>,
    pub date: Option<String>,
    pub order: Option<OrderMetadata>,
    pub assignee: Option<String>,
    pub assignee_exclusive: bool,
    pub wbs: Vec<String>,
    pub definitions: Vec<DefinitionId>,
}

#[derive(Debug, Clone)]
pub struct Forest {
    pub files: Vec<SourceFile>,
    pub definitions: Vec<Definition>,
    pub chores: Vec<Chore>,
    pub amp_occurrences: Vec<AmpOccurrence>,
    pub issues: Vec<CheckIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmpOccurrence {
    pub file: FileId,
    pub line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub raw: String,
    pub definition: DefinitionId,
    pub is_declaration: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIssue {
    pub kind: CheckIssueKind,
    pub file: Option<FileId>,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckIssueKind {
    UnresolvedAmpPath,
    AmbiguousAmpPath,
    AmbiguousDefinition,
    UnresolvedAssignee,
    AmbiguousAssignee,
    RelativeDefinitionWithoutParent,
    WbsWithoutDefinition,
    MarkdownParsing,
    ConflictingExclusiveOrder,
    EmptyAmpPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputedOrder {
    pub value: u32,
    pub exclusive: bool,
    pub conflict: bool,
}

pub fn computed_chore_order(forest: &Forest, chore: &Chore) -> Option<ComputedOrder> {
    computed_chore_order_for_definitions(&forest.definitions, chore)
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub include_amp_metadata: bool,
    pub status: Option<Status>,
    pub amp_tags: Vec<String>,
    pub extensions: Vec<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_amp_metadata: true,
            status: None,
            amp_tags: Vec::new(),
            extensions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSummary {
    pub files_written: usize,
}

pub fn export_forest(
    forest: &Forest,
    destination: impl AsRef<Path>,
    options: &ExportOptions,
) -> Result<ExportSummary> {
    let destination = export_destination(forest, destination.as_ref())?;
    let mut files_written = 0;

    for file in forest
        .files
        .iter()
        .filter(|file| export_file_matches(forest, file, options))
    {
        let relative = file.path.strip_prefix(&file.root).unwrap_or(&file.path);
        let output_path = destination.join(relative);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if options.include_amp_metadata {
            fs::write(&output_path, &file.bytes)?;
        } else {
            fs::write(&output_path, parse::strip_amp_metadata(&file.text))?;
        }
        files_written += 1;
    }

    Ok(ExportSummary { files_written })
}

fn export_destination(forest: &Forest, destination: &Path) -> Result<PathBuf> {
    let destination = absolute_path(destination)?;
    for root in forest.files.iter().map(|file| file.root.as_path()) {
        if destination == root || destination.starts_with(root) {
            return Err(format!(
                "export destination {} cannot be inside Grove path {}",
                destination.display(),
                root.display()
            )
            .into());
        }
    }
    Ok(destination)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(clean_path(&joined))
}

fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            _ => cleaned.push(component.as_os_str()),
        }
    }
    cleaned
}

fn export_file_matches(forest: &Forest, file: &SourceFile, options: &ExportOptions) -> bool {
    if !extension_matches(&file.path, &options.extensions) {
        return false;
    }
    if options.status.is_none() && options.amp_tags.is_empty() {
        return true;
    }
    forest
        .chores
        .iter()
        .filter(|chore| chore.file == file.id)
        .any(|chore| export_chore_matches(forest, chore, options))
}

fn extension_matches(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    let Some(file_ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    extensions.iter().any(|ext| {
        let ext = ext.trim_start_matches('.');
        file_ext.eq_ignore_ascii_case(ext)
    })
}

fn export_chore_matches(forest: &Forest, chore: &Chore, options: &ExportOptions) -> bool {
    if options
        .status
        .is_some_and(|status| chore.status != Some(status))
    {
        return false;
    }
    if options.amp_tags.is_empty() {
        return true;
    }
    options.amp_tags.iter().any(|tag| {
        let tag = normalize_amp_path(tag);
        chore.definitions.iter().any(|id| {
            let path = forest.definitions[id.0].path.as_str();
            path == tag || path.ends_with(&format!(":{tag}")) || amp_path_tail(path) == tag
        })
    })
}

#[derive(Debug, Clone)]
struct ParsedLine {
    file: FileId,
    line: usize,
    column: usize,
    /// Byte range of the lossy UTF-8 source view (excluding line endings).
    /// Keeping this as an offset avoids retaining another allocation for
    /// every parsed chore; the original bytes remain in `SourceFile::bytes`.
    span: (usize, usize),
    metadata: Metadata,
    inherited_refs: Vec<String>,
    is_chore: bool,
}

pub fn build_forest(config: &Config) -> Result<Forest> {
    let files = load_files(config)?;
    Ok(build_forest_from_files(files))
}

pub fn build_forest_with_reporter(
    config: &Config,
    verbose: u8,
    report: impl FnMut(&Path),
) -> Result<Forest> {
    let files = load_files_with_reporter(config, verbose, report)?;
    Ok(build_forest_from_files(files))
}

fn build_forest_from_files(files: Vec<SourceFile>) -> Forest {
    let mut builder = ForestBuilder::new(files);
    builder.parse_files();
    builder.finish()
}

pub fn build_forest_with_overlays(
    config: &Config,
    overlays: &HashMap<PathBuf, String>,
) -> Result<Forest> {
    let mut files = load_files(config)?;
    for file in &mut files {
        let absolute = file
            .path
            .canonicalize()
            .unwrap_or_else(|_| file.path.clone());
        if let Some(text) = overlays.get(&absolute) {
            file.bytes = text.as_bytes().to_vec();
            file.text = Arc::new(text.clone());
        }
    }
    for (path, text) in overlays {
        if files.iter().any(|file| {
            file.path
                .canonicalize()
                .unwrap_or_else(|_| file.path.clone())
                == *path
        }) {
            continue;
        }
        let Some((grove, root)) = config.groves.iter().enumerate().find_map(|(index, grove)| {
            let root = grove
                .root
                .canonicalize()
                .unwrap_or_else(|_| grove.root.clone());
            path.starts_with(&root).then_some((index, root))
        }) else {
            continue;
        };
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let accepted = if config.groves[grove].extensions.is_empty() {
            matches!(
                extension,
                "md" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "rb" | "rs" | "zig"
            )
        } else {
            config.groves[grove]
                .extensions
                .iter()
                .any(|item| item.trim_start_matches('.').eq_ignore_ascii_case(extension))
        };
        if accepted {
            files.push(SourceFile {
                id: FileId(files.len()),
                grove,
                root,
                path: path.clone(),
                bytes: text.as_bytes().to_vec(),
                text: Arc::new(text.clone()),
            });
        }
    }
    Ok(build_forest_from_files(files))
}

struct ForestBuilder {
    files: Vec<SourceFile>,
    definitions: Vec<Definition>,
    definition_by_path: HashMap<String, DefinitionId>,
    definition_declarations: HashMap<String, Vec<DefinitionDeclaration>>,
    parsed_lines: Vec<ParsedLine>,
    folder_metadata: HashMap<(usize, PathBuf), Metadata>,
    file_metadata: HashMap<FileId, Metadata>,
    filesystem_definition_by_file: HashMap<FileId, DefinitionId>,
    issues: Vec<CheckIssue>,
}

#[derive(Debug, Clone, Copy)]
struct DefinitionDeclaration {
    file: FileId,
    line: usize,
    exclusive: bool,
    is_assignee: bool,
}

impl ForestBuilder {
    fn new(files: Vec<SourceFile>) -> Self {
        Self {
            files,
            definitions: Vec::new(),
            definition_by_path: HashMap::new(),
            definition_declarations: HashMap::new(),
            parsed_lines: Vec::new(),
            folder_metadata: HashMap::new(),
            file_metadata: HashMap::new(),
            filesystem_definition_by_file: HashMap::new(),
            issues: Vec::new(),
        }
    }

    fn parse_files(&mut self) {
        for index in 0..self.files.len() {
            self.parse_file(FileId(index));
        }
        self.validate_definitions();
        self.validate_assignees();
        self.resolve_bare_assignees();
        self.build_filesystem_definitions();
    }

    fn parse_file(&mut self, file_id: FileId) {
        let file = &self.files[file_id.0];
        let path = file.path.clone();
        let grove = file.grove;
        let text = Arc::clone(&file.text);
        let is_markdown = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        let is_folder_metadata = path.file_name().and_then(|name| name.to_str()) == Some("&.md");
        let mut heading_stack: Vec<(usize, Metadata)> = Vec::new();
        let mut bullet_stack: Vec<(usize, Metadata)> = Vec::new();
        let mut folder_md = Metadata::default();
        let mut markdown_state = parse::MarkdownState::default();
        let filesystem_date = date_from_file_path(&path, &file.root);

        let mut line_start = 0;
        for (line_index, line_with_cr) in text.split_terminator('\n').enumerate() {
            let raw_line = line_with_cr.strip_suffix('\r').unwrap_or(line_with_cr);
            let line_no = line_index + 1;
            let current_line_start = line_start;
            line_start += line_with_cr.len() + 1;
            let Some(content) = parse::content_line(&raw_line, is_markdown, &path) else {
                continue;
            };
            // `content_line` may select only the comment payload in a source
            // line. Keep the exact payload range rather than the whole line.
            let content_offset = raw_line.find(content.text).unwrap_or(0);
            let metadata_text;
            let metadata_source = if is_markdown {
                let Some(visible) =
                    parse::markdown_visible_line_with_issues(content.text, &mut markdown_state)
                else {
                    continue;
                };
                for issue in visible.issues {
                    self.issues.push(CheckIssue {
                        kind: CheckIssueKind::MarkdownParsing,
                        file: Some(file_id),
                        line: Some(line_no),
                        message: issue.message,
                    });
                }
                metadata_text = visible.text;
                metadata_text.as_str()
            } else {
                content.text
            };
            let mut metadata = extract_metadata(metadata_source);
            for _ in 0..metadata.empty_amp_paths {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::EmptyAmpPath,
                    file: Some(file_id),
                    line: Some(line_no),
                    message: "empty AmpPath is not allowed and was omitted".to_string(),
                });
            }
            if let Some(date) = filesystem_date.as_ref()
                && metadata.date.as_ref().is_none_or(|current| date < current)
            {
                metadata.date = Some(date.clone());
            }
            if !metadata.wbs.is_empty() && metadata.definitions.is_empty() {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::WbsWithoutDefinition,
                    file: Some(file_id),
                    line: Some(line_no),
                    message: format!(
                        "WBS metadata {} must be specified on a line with a Definition",
                        metadata
                            .wbs
                            .iter()
                            .map(|wbs| format!("&?{wbs}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
            if is_folder_metadata {
                folder_md.merge(&metadata);
            }

            if is_markdown {
                if let Some(level) = parse::heading_level(metadata_source) {
                    heading_stack.truncate(level.saturating_sub(1));
                    bullet_stack.clear();
                } else if let Some(indent) = parse::markdown_item_indent(metadata_source) {
                    while bullet_stack.last().is_some_and(|(prev, _)| *prev >= indent) {
                        bullet_stack.pop();
                    }
                }
            }

            for amp in metadata.definitions.iter() {
                let inherited =
                    self.current_definition_context(file_id, &heading_stack, &bullet_stack);
                if !is_absolute_definition_path(amp) && inherited.is_empty() {
                    self.issues.push(CheckIssue {
                        kind: CheckIssueKind::RelativeDefinitionWithoutParent,
                        file: Some(file_id),
                        line: Some(line_no),
                        message: format!(
                            "relative Definition {amp} has no higher-level Definition to extend"
                        ),
                    });
                }
                let path = resolve_definition_path(amp, inherited.last());
                let exclusive = definition_is_exclusive(amp);
                let is_assignee = definition_is_assignee(amp);
                self.definition_declarations
                    .entry(path.clone())
                    .or_default()
                    .push(DefinitionDeclaration {
                        file: file_id,
                        line: line_no,
                        exclusive,
                        is_assignee,
                    });
                self.upsert_definition(
                    &path,
                    false,
                    Some(file_id),
                    Some(line_no),
                    &metadata,
                    exclusive,
                    is_assignee,
                );
            }

            if metadata.is_chore_marker() {
                let mut inherited_refs = Vec::new();
                for (_, md) in heading_stack.iter().chain(bullet_stack.iter()) {
                    inherited_refs.extend(md.references.iter().cloned());
                    inherited_refs.extend(md.definitions.iter().cloned());
                }
                self.parsed_lines.push(ParsedLine {
                    file: file_id,
                    line: line_no,
                    column: content.column,
                    span: (
                        current_line_start + content_offset,
                        current_line_start + content_offset + content.text.len(),
                    ),
                    metadata: metadata.clone(),
                    inherited_refs,
                    is_chore: true,
                });
            }

            if line_no == 1 && !metadata.is_empty() {
                self.file_metadata.insert(file_id, metadata.clone());
            }

            if is_markdown {
                if let Some(level) = parse::heading_level(metadata_source) {
                    heading_stack.push((level, metadata));
                } else if let Some(indent) = parse::markdown_item_indent(metadata_source) {
                    bullet_stack.push((indent, metadata));
                }
            }
        }

        if is_markdown {
            for issue in markdown_state.finish_issues() {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::MarkdownParsing,
                    file: Some(file_id),
                    line: None,
                    message: issue.message,
                });
            }
        }

        if is_folder_metadata
            && !folder_md.is_empty()
            && let Some(parent) = path.parent()
        {
            self.folder_metadata
                .insert((grove, parent.to_path_buf()), folder_md);
        }
    }

    fn current_definition_context(
        &self,
        file_id: FileId,
        heading_stack: &[(usize, Metadata)],
        bullet_stack: &[(usize, Metadata)],
    ) -> Vec<String> {
        let mut refs = self.folder_context(file_id);
        if let Some(md) = self.file_metadata.get(&file_id) {
            refs.extend(
                md.definitions
                    .iter()
                    .map(|definition| resolve_definition_path(definition, None)),
            );
        }
        for (_, md) in heading_stack.iter().chain(bullet_stack.iter()) {
            refs.extend(
                md.definitions
                    .iter()
                    .map(|definition| resolve_definition_path(definition, None)),
            );
        }
        refs
    }

    fn folder_context(&self, file_id: FileId) -> Vec<String> {
        let file = &self.files[file_id.0];
        let mut refs = Vec::new();
        let mut current = file.path.parent();
        while let Some(dir) = current {
            if let Some(md) = self.folder_metadata.get(&(file.grove, dir.to_path_buf())) {
                refs.extend(md.references.iter().cloned());
                refs.extend(
                    md.definitions
                        .iter()
                        .map(|definition| resolve_definition_path(definition, None)),
                );
            }
            current = dir.parent();
        }
        refs.reverse();
        refs
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_definition(
        &mut self,
        path: &str,
        is_phony: bool,
        file: Option<FileId>,
        line: Option<usize>,
        md: &Metadata,
        exclusive: bool,
        is_assignee: bool,
    ) -> DefinitionId {
        if let Some(id) = self.definition_by_path.get(path).copied() {
            let def = &mut self.definitions[id.0];
            if exclusive && !def.exclusive {
                def.file = file;
                def.line = line;
                def.date = md.date.clone();
                def.order = md.order;
                def.assignee = md.assignee.clone();
                def.assignee_exclusive = md.assignee_exclusive;
                def.wbs = md.wbs.clone();
            }
            def.is_phony &= is_phony;
            def.exclusive |= exclusive;
            def.is_assignee |= is_assignee;
            def.file = def.file.or(file);
            def.line = def.line.or(line);
            def.date = def.date.clone().or_else(|| md.date.clone());
            def.order = def.order.or(md.order);
            if md.assignee_exclusive {
                def.assignee = md.assignee.clone();
                def.assignee_exclusive = true;
            } else if def.assignee.is_none() {
                def.assignee = md.assignee.clone();
            }
            for wbs in &md.wbs {
                if !def.wbs.contains(wbs) {
                    def.wbs.push(wbs.clone());
                }
            }
            return id;
        }
        let id = DefinitionId(self.definitions.len());
        self.definition_by_path.insert(path.to_string(), id);
        self.definitions.push(Definition {
            id,
            path: path.to_string(),
            is_phony,
            exclusive,
            is_assignee,
            file,
            line,
            date: md.date.clone(),
            order: md.order,
            assignee: md.assignee.clone(),
            assignee_exclusive: md.assignee_exclusive,
            wbs: md.wbs.clone(),
            definitions: Vec::new(),
        });
        id
    }

    fn validate_definitions(&mut self) {
        for (path, declarations) in &self.definition_declarations {
            if declarations.len() < 2 {
                continue;
            }
            let exclusive = declarations.iter().filter(|decl| decl.exclusive).count();
            if exclusive == 1 {
                self.issues.retain(|issue| {
                    issue.kind != CheckIssueKind::RelativeDefinitionWithoutParent
                        || !declarations.iter().any(|declaration| {
                            issue.file == Some(declaration.file)
                                && issue.line == Some(declaration.line)
                        })
                });
                continue;
            }
            for declaration in declarations {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::AmbiguousDefinition,
                    file: Some(declaration.file),
                    line: Some(declaration.line),
                    message: format!(
                        "Definition {path} is declared {} times; mark exactly one declaration exclusive with ^",
                        declarations.len()
                    ),
                });
            }
        }
    }

    fn validate_assignees(&mut self) {
        let mut assignees: HashMap<&str, Vec<&DefinitionDeclaration>> = HashMap::new();
        for (path, declarations) in &self.definition_declarations {
            for declaration in declarations.iter().filter(|decl| decl.is_assignee) {
                assignees.entry(path).or_default().push(declaration);
            }
        }
        for line in &self.parsed_lines {
            let Some(name) = line.metadata.assignee.as_deref() else {
                continue;
            };
            match assignees.get(name).map(Vec::as_slice).unwrap_or_default() {
                [] => self.issues.push(CheckIssue {
                    kind: CheckIssueKind::UnresolvedAssignee,
                    file: Some(line.file),
                    line: Some(line.line),
                    message: format!("assignee @{name} has no matching &&@{name} Definition"),
                }),
                declarations
                    if declarations.len() > 1
                        && declarations.iter().filter(|decl| decl.exclusive).count() != 1 =>
                {
                    self.issues.push(CheckIssue {
                        kind: CheckIssueKind::AmbiguousAssignee,
                        file: Some(line.file),
                        line: Some(line.line),
                        message: format!("assignee @{name} matches multiple &&@{name} Definitions"),
                    });
                }
                _ => {}
            }
        }
    }

    fn resolve_bare_assignees(&mut self) {
        let assignees = self
            .definition_declarations
            .iter()
            .filter(|(_, declarations)| declarations.iter().any(|item| item.is_assignee))
            .map(|(path, _)| path.clone())
            .collect::<HashSet<_>>();
        for line in &mut self.parsed_lines {
            if line.metadata.assignee.is_some() {
                continue;
            }
            line.metadata.assignee = line
                .metadata
                .bare_assignees
                .iter()
                .rev()
                .find(|name| assignees.contains(*name))
                .cloned();
        }
    }

    fn finish(mut self) -> Forest {
        let mut chores = Vec::new();
        let parsed_lines = std::mem::take(&mut self.parsed_lines);
        self.inject_definition_relationships(&parsed_lines);
        for line in parsed_lines.into_iter().filter(|line| line.is_chore) {
            let mut inherited_amp_paths = self.folder_context(line.file);
            if let Some(md) = self.file_metadata.get(&line.file) {
                inherited_amp_paths.extend(md.references.iter().cloned());
                inherited_amp_paths.extend(md.definitions.iter().cloned());
            }
            inherited_amp_paths.extend(line.inherited_refs);

            let mut ids = Vec::new();
            let mut seen = HashSet::new();
            if let Some(id) = self.filesystem_definition_by_file.get(&line.file).copied()
                && seen.insert(id)
            {
                ids.push(id);
            }
            let inherited_metadata = Metadata::default();
            for amp in inherited_amp_paths {
                push_resolved_amp(
                    &mut self,
                    &mut ids,
                    &mut seen,
                    &amp,
                    &inherited_metadata,
                    line.file,
                    line.line,
                );
            }
            for amp in line.metadata.references.iter() {
                if is_injection_amp_path(amp) {
                    continue;
                }
                push_resolved_amp(
                    &mut self,
                    &mut ids,
                    &mut seen,
                    amp,
                    &line.metadata,
                    line.file,
                    line.line,
                );
            }
            for amp in line.metadata.definitions.iter() {
                push_resolved_amp(
                    &mut self,
                    &mut ids,
                    &mut seen,
                    amp,
                    &line.metadata,
                    line.file,
                    line.line,
                );
            }
            add_existing_ancestor_definitions(&self, &mut ids, &mut seen);
            add_injected_definitions(&self, &mut ids, &mut seen);
            add_existing_ancestor_definitions(&self, &mut ids, &mut seen);

            let chore = Chore {
                file: line.file,
                line: line.line,
                column: line.column,
                text: self.files[line.file.0].text[line.span.0..line.span.1].to_string(),
                status: line.metadata.status,
                date: line.metadata.date,
                order: line.metadata.order,
                assignee: line.metadata.assignee,
                assignee_exclusive: line.metadata.assignee_exclusive,
                wbs: line.metadata.wbs,
                definitions: ids,
            };
            if computed_chore_order_for_definitions(&self.definitions, &chore)
                .is_some_and(|order| order.conflict)
            {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::ConflictingExclusiveOrder,
                    file: Some(chore.file),
                    line: Some(chore.line),
                    message: "Chore has multiple related exclusive order values".to_string(),
                });
            }
            chores.push(chore);
        }
        self.add_filename_chores(&mut chores);

        let amp_occurrences = self.build_amp_occurrences();
        Forest {
            files: self.files,
            definitions: self.definitions,
            chores,
            amp_occurrences,
            issues: self.issues,
        }
    }

    fn build_amp_occurrences(&self) -> Vec<AmpOccurrence> {
        let mut occurrences = Vec::new();
        for file in &self.files {
            let is_markdown = file
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            let mut markdown_state = parse::MarkdownState::default();
            for (line_index, raw_line) in file.text.lines().enumerate() {
                let Some(content) = parse::content_line(raw_line, is_markdown, &file.path) else {
                    continue;
                };
                let base = raw_line.find(content.text).unwrap_or(0);
                let visible;
                let source = if is_markdown {
                    let Some(line) =
                        parse::markdown_visible_line_with_issues(content.text, &mut markdown_state)
                    else {
                        continue;
                    };
                    visible = line.text;
                    visible.as_str()
                } else {
                    content.text
                };
                for (start, end, raw) in amp_tokens(source) {
                    let metadata = extract_metadata(raw);
                    let (amp, is_declaration) = if let Some(amp) = metadata.definitions.first() {
                        (amp.as_str(), true)
                    } else if let Some(amp) = metadata.references.first() {
                        (amp.as_str(), false)
                    } else {
                        continue;
                    };
                    let definition = if is_declaration {
                        self.definitions.iter().find(|definition| {
                            definition.file == Some(file.id)
                                && definition.line == Some(line_index + 1)
                                && (definition.path == resolve_definition_path(amp, None)
                                    || definition
                                        .path
                                        .ends_with(&format!(":{}", normalize_amp_path(amp))))
                        })
                    } else {
                        resolved_definition(&self.definitions, amp)
                    };
                    if let Some(definition) = definition {
                        occurrences.push(AmpOccurrence {
                            file: file.id,
                            line: line_index + 1,
                            start_column: base + start + 1,
                            end_column: base + end + 1,
                            raw: raw.to_string(),
                            definition: definition.id,
                            is_declaration,
                        });
                    }
                }
            }
        }
        occurrences
    }

    fn add_filename_chores(&mut self, chores: &mut Vec<Chore>) {
        let files = self.files.clone();
        for file in files {
            let Some(filesystem_definition) =
                self.filesystem_definition_by_file.get(&file.id).copied()
            else {
                continue;
            };
            let Some(stem) = file.path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(status) = filename_status(stem) else {
                continue;
            };

            let mut ids = Vec::new();
            let mut seen = HashSet::new();
            if seen.insert(filesystem_definition) {
                ids.push(filesystem_definition);
            }
            let inherited_metadata = Metadata::default();
            let mut inherited_amp_paths = self.folder_context(file.id);
            if let Some(metadata) = self.file_metadata.get(&file.id) {
                inherited_amp_paths.extend(metadata.references.iter().cloned());
                inherited_amp_paths.extend(metadata.definitions.iter().cloned());
            }
            for amp in inherited_amp_paths {
                push_resolved_amp(
                    self,
                    &mut ids,
                    &mut seen,
                    &amp,
                    &inherited_metadata,
                    file.id,
                    1,
                );
            }
            add_existing_ancestor_definitions(self, &mut ids, &mut seen);
            add_injected_definitions(self, &mut ids, &mut seen);
            add_existing_ancestor_definitions(self, &mut ids, &mut seen);

            chores.push(Chore {
                file: file.id,
                line: 1,
                column: 1,
                text: format!(
                    "- [{}] {stem}",
                    if status == Status::Todo { " " } else { "x" }
                ),
                status: Some(status),
                date: date_from_file_path(&file.path, &file.root),
                order: None,
                assignee: None,
                assignee_exclusive: false,
                wbs: Vec::new(),
                definitions: ids,
            });
        }
    }

    fn build_filesystem_definitions(&mut self) {
        let files = self.files.clone();
        for file in files {
            if file.path.file_name().and_then(|name| name.to_str()) == Some("&.md") {
                continue;
            }
            let mut directories = Vec::new();
            let mut current = file.path.parent();
            while let Some(dir) = current {
                if !dir.starts_with(&file.root) {
                    break;
                }
                directories.push(dir.to_path_buf());
                if dir == file.root {
                    break;
                }
                current = dir.parent();
            }
            directories.reverse();

            let mut active: Option<(String, PathBuf)> = None;
            for directory in directories {
                let Some(metadata) = self
                    .folder_metadata
                    .get(&(file.grove, directory.clone()))
                    .cloned()
                else {
                    continue;
                };
                if metadata.definitions.is_empty() {
                    continue;
                }
                let inherited = active.as_ref().map(|(base, base_dir)| {
                    append_filesystem_components(base, base_dir, &directory)
                });
                active = None;
                for definition in metadata.definitions.iter() {
                    if definition_is_filesystem(definition) {
                        let path = resolve_definition_path(definition, inherited.as_ref());
                        active = Some((path, directory.clone()));
                    }
                }
            }

            if let Some(metadata) = self.file_metadata.get(&file.id).cloned()
                && !metadata.definitions.is_empty()
            {
                let inherited = active.as_ref().map(|(base, base_dir)| {
                    append_filesystem_components(
                        base,
                        base_dir,
                        file.path.parent().unwrap_or(&file.root),
                    )
                });
                active = None;
                for definition in metadata.definitions.iter() {
                    if definition_is_filesystem(definition) {
                        active = Some((
                            resolve_definition_path(definition, inherited.as_ref()),
                            file.path.parent().unwrap_or(&file.root).to_path_buf(),
                        ));
                    }
                }
            }

            let Some((base, base_dir)) = active else {
                continue;
            };
            let relative = file.path.strip_prefix(&base_dir).unwrap_or(&file.path);
            let mut path = base;
            let components = relative.components().collect::<Vec<_>>();
            for (index, component) in components.iter().enumerate() {
                let raw = component.as_os_str().to_string_lossy();
                let value = if index + 1 == components.len() {
                    Path::new(raw.as_ref())
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or(raw.as_ref())
                        .to_string()
                } else {
                    raw.to_string()
                };
                let part = filesystem_amp_part(&value);
                if part.is_empty() {
                    continue;
                }
                path = format!("{path}:{part}");
                let id = self.upsert_definition(
                    &path,
                    false,
                    Some(file.id),
                    Some(1),
                    &Metadata::default(),
                    false,
                    false,
                );
                if index + 1 == components.len() {
                    self.filesystem_definition_by_file.insert(file.id, id);
                }
            }
        }
    }

    fn inject_definition_relationships(&mut self, lines: &[ParsedLine]) {
        for line in lines {
            let targets = line
                .metadata
                .references
                .iter()
                .filter(|amp| is_injection_amp_path(amp))
                .cloned()
                .collect::<Vec<_>>();
            if targets.is_empty() {
                continue;
            }
            let sources = line
                .metadata
                .references
                .iter()
                .filter(|amp| !is_injection_amp_path(amp))
                .chain(line.metadata.definitions.iter())
                .cloned()
                .collect::<Vec<_>>();
            let mut source_ids = Vec::new();
            for source in sources {
                let id = if source.starts_with("&&") {
                    let path = resolve_definition_path(&source, None);
                    self.upsert_definition(
                        &path,
                        false,
                        Some(line.file),
                        Some(line.line),
                        &line.metadata,
                        definition_is_exclusive(&source),
                        definition_is_assignee(&source),
                    )
                } else {
                    self.resolve_reference(&source, &line.metadata, line.file, line.line)
                };
                if !source_ids.contains(&id) {
                    source_ids.push(id);
                }
            }
            for target in targets {
                let target_amp = target.trim_end_matches('&');
                let target_id =
                    self.resolve_reference(target_amp, &line.metadata, line.file, line.line);
                let definition = &mut self.definitions[target_id.0];
                for source_id in &source_ids {
                    if *source_id != target_id && !definition.definitions.contains(source_id) {
                        definition.definitions.push(*source_id);
                    }
                }
            }
        }
    }

    fn resolve_reference(
        &mut self,
        amp: &str,
        md: &Metadata,
        file: FileId,
        line: usize,
    ) -> DefinitionId {
        let reference = normalize_amp_path(amp);
        if let Some(id) = self.definition_by_path.get(&reference).copied() {
            return id;
        }
        let suffix = format!(":{reference}");
        let direct_matches = self
            .definitions
            .iter()
            .filter(|def| {
                def.path == reference
                    || def.path.ends_with(&suffix)
                    || amp_path_tail(&def.path) == reference
            })
            .map(|def| def.id)
            .collect::<Vec<_>>();
        let matches = if direct_matches.is_empty() {
            self.definitions
                .iter()
                .filter(|def| amp_path_partial_match(&reference, &def.path))
                .map(|def| def.id)
                .collect::<Vec<_>>()
        } else {
            direct_matches
        };
        match matches.as_slice() {
            [id] => *id,
            [] => {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::UnresolvedAmpPath,
                    file: Some(file),
                    line: Some(line),
                    message: format!("AmpPath {amp} could not be resolved to a Definition"),
                });
                self.upsert_definition(&reference, true, Some(file), Some(line), md, false, false)
            }
            _ => {
                let candidates = matches
                    .iter()
                    .map(|id| self.definitions[id.0].path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::AmbiguousAmpPath,
                    file: Some(file),
                    line: Some(line),
                    message: format!("AmpPath {amp} is ambiguous; candidates: {candidates}"),
                });
                self.upsert_definition(&reference, true, Some(file), Some(line), md, false, false)
            }
        }
    }
}

fn is_injection_amp_path(amp: &str) -> bool {
    !amp.starts_with("&&") && amp.ends_with('&') && amp.len() > 2
}

fn add_injected_definitions(
    builder: &ForestBuilder,
    ids: &mut Vec<DefinitionId>,
    seen: &mut HashSet<DefinitionId>,
) {
    let mut index = 0;
    while index < ids.len() {
        let id = ids[index];
        for related in &builder.definitions[id.0].definitions {
            if seen.insert(*related) {
                ids.push(*related);
            }
        }
        index += 1;
    }
}

fn add_existing_ancestor_definitions(
    builder: &ForestBuilder,
    ids: &mut Vec<DefinitionId>,
    seen: &mut HashSet<DefinitionId>,
) {
    let paths = ids
        .iter()
        .map(|id| builder.definitions[id.0].path.clone())
        .collect::<Vec<_>>();
    for path in paths {
        let mut current = path.as_str();
        while let Some(parent) = amp_path_parent(current) {
            if let Some(id) = builder.definition_by_path.get(parent).copied()
                && seen.insert(id)
            {
                ids.push(id);
            }
            current = parent;
        }
    }
}

fn computed_chore_order_for_definitions(
    definitions: &[Definition],
    chore: &Chore,
) -> Option<ComputedOrder> {
    let orders = chore
        .definitions
        .iter()
        .filter_map(|id| definitions[id.0].order)
        .collect::<Vec<_>>();
    let exclusive = orders
        .iter()
        .filter(|order| order.exclusive)
        .copied()
        .collect::<Vec<_>>();
    if !exclusive.is_empty() {
        let value = exclusive.iter().map(|order| order.value).min()?;
        let mut unique = exclusive
            .iter()
            .map(|order| order.value)
            .collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        return Some(ComputedOrder {
            value,
            exclusive: true,
            conflict: unique.len() > 1,
        });
    }
    orders
        .iter()
        .map(|order| order.value)
        .min()
        .map(|value| ComputedOrder {
            value,
            exclusive: false,
            conflict: false,
        })
}

fn push_resolved_amp(
    builder: &mut ForestBuilder,
    ids: &mut Vec<DefinitionId>,
    seen: &mut HashSet<DefinitionId>,
    amp: &str,
    metadata: &Metadata,
    file: FileId,
    line: usize,
) {
    let id = if amp.starts_with("&&") {
        let path = resolve_definition_path(amp, None);
        builder.upsert_definition(
            &path,
            false,
            Some(file),
            Some(line),
            metadata,
            definition_is_exclusive(amp),
            definition_is_assignee(amp),
        )
    } else {
        builder.resolve_reference(amp, metadata, file, line)
    };
    if seen.insert(id) {
        ids.push(id);
    }
}

fn resolve_definition_path(amp: &str, parent: Option<&String>) -> String {
    let raw = amp
        .trim_start_matches('&')
        .trim_start_matches('^')
        .trim_end_matches('&');
    if let Some(assignee) = raw.strip_prefix('@') {
        return normalize_amp_path(assignee);
    }
    if raw.starts_with(':') {
        normalize_amp_path(raw)
    } else if let Some(parent) = parent {
        let normalized = normalize_amp_path(raw);
        if parent.is_empty() {
            normalized
        } else {
            format!("{parent}:{normalized}")
        }
    } else {
        normalize_amp_path(raw)
    }
}

fn is_absolute_definition_path(amp: &str) -> bool {
    let raw = amp.trim_start_matches('&').trim_start_matches('^');
    raw.starts_with(':') || raw.starts_with('@')
}

fn definition_is_exclusive(amp: &str) -> bool {
    amp.trim_start_matches('&').starts_with('^')
}

fn definition_is_assignee(amp: &str) -> bool {
    amp.trim_start_matches('&')
        .trim_start_matches('^')
        .starts_with('@')
}

fn definition_is_filesystem(amp: &str) -> bool {
    amp.starts_with("&&") && amp.ends_with('&') && amp.len() > 3
}

fn date_from_file_path(path: &Path, root: &Path) -> Option<String> {
    let mut dates = Vec::new();
    if let Some(root_name) = root.file_name().and_then(|name| name.to_str())
        && let Some(date) = parse::date_in_path_component(root_name)
    {
        dates.push(date);
    }
    let relative = path.strip_prefix(root).unwrap_or(path);
    for component in relative.components() {
        if let Some(value) = component.as_os_str().to_str()
            && let Some(date) = parse::date_in_path_component(value)
        {
            dates.push(date);
        }
    }
    dates.into_iter().min()
}

fn amp_tokens(line: &str) -> Vec<(usize, usize, &str)> {
    let bytes = line.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'&' {
            index += 1;
            continue;
        }
        let start = index;
        if line[start..].starts_with("&[[")
            && let Some(close) = line[start + 3..].find("]]")
        {
            index = start + 3 + close + 2;
            if bytes.get(index) == Some(&b'&') {
                index += 1;
            }
            result.push((start, index, &line[start..index]));
            continue;
        }
        index += 1;
        let mut quoted = false;
        while index < bytes.len() {
            let ch = line[index..].chars().next().unwrap();
            if ch == '`' {
                quoted = !quoted;
            } else if !(quoted
                || ch.is_ascii_alphanumeric()
                || matches!(ch, '_' | '&' | ':' | '#' | '^' | '@' | '?' | '+'))
            {
                break;
            }
            index += ch.len_utf8();
        }
        result.push((start, index, &line[start..index]));
    }
    result
}

fn resolved_definition<'a>(definitions: &'a [Definition], amp: &str) -> Option<&'a Definition> {
    let reference = normalize_amp_path(amp.trim_end_matches('&'));
    definitions
        .iter()
        .find(|definition| definition.path == reference)
        .or_else(|| {
            let suffix = format!(":{reference}");
            let matches = definitions
                .iter()
                .filter(|definition| {
                    definition.path.ends_with(&suffix)
                        || amp_path_tail(&definition.path) == reference
                })
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                Some(matches[0])
            } else if matches.is_empty() {
                let partial = definitions
                    .iter()
                    .filter(|definition| amp_path_partial_match(&reference, &definition.path))
                    .collect::<Vec<_>>();
                (partial.len() == 1).then(|| partial[0])
            } else {
                None
            }
        })
}

fn filename_status(stem: &str) -> Option<Status> {
    let mut chars = stem.chars();
    let first = chars.next()?;
    if first.is_lowercase() && chars.any(char::is_uppercase) {
        Some(Status::Todo)
    } else if first.is_uppercase() {
        Some(Status::Done)
    } else {
        None
    }
}

fn append_filesystem_components(base: &str, base_dir: &Path, directory: &Path) -> String {
    let mut path = base.to_string();
    for component in directory
        .strip_prefix(base_dir)
        .unwrap_or(directory)
        .components()
    {
        let part = filesystem_amp_part(&component.as_os_str().to_string_lossy());
        if !part.is_empty() {
            path.push(':');
            path.push_str(&part);
        }
    }
    path
}

fn filesystem_amp_part(value: &str) -> String {
    let value = value.to_lowercase();
    if value.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        value
    } else {
        format!("`{value}`")
    }
}

fn normalize_amp_path(amp: &str) -> String {
    amp.trim_start_matches('&')
        .trim_start_matches('^')
        .trim_start_matches('@')
        .trim_start_matches(':')
        .trim_matches(':')
        .to_lowercase()
}

pub fn amp_path_depth(path: &str) -> usize {
    amp_path_separator_indices(path).len()
}

fn amp_path_tail(path: &str) -> &str {
    amp_path_separator_indices(path)
        .last()
        .map(|index| &path[index + 1..])
        .unwrap_or(path)
}

fn amp_path_parent(path: &str) -> Option<&str> {
    amp_path_separator_indices(path)
        .last()
        .map(|index| &path[..*index])
        .filter(|parent| !parent.is_empty())
}

fn amp_path_separator_indices(path: &str) -> Vec<usize> {
    let mut quoted = false;
    path.char_indices()
        .filter_map(|(index, ch)| {
            if ch == '`' {
                quoted = !quoted;
                None
            } else if ch == ':' && !quoted {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

fn amp_path_parts(path: &str) -> Vec<&str> {
    let separators = amp_path_separator_indices(path);
    let mut parts = Vec::with_capacity(separators.len() + 1);
    let mut start = 0;
    for separator in separators {
        parts.push(&path[start..separator]);
        start = separator + 1;
    }
    parts.push(&path[start..]);
    parts
}

fn amp_path_partial_match(reference: &str, definition: &str) -> bool {
    let reference = amp_path_parts(reference);
    let definition = amp_path_parts(definition);
    if reference.len() < 2 || reference.len() >= definition.len() {
        return false;
    }
    if reference.last() != definition.last() {
        return false;
    }

    let mut definition_index = definition.len() - 1;
    for reference_part in reference[..reference.len() - 1].iter().rev() {
        let Some(index) = definition[..definition_index]
            .iter()
            .rposition(|definition_part| definition_part == reference_part)
        else {
            return false;
        };
        definition_index = index;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_unique_suffix_reference_to_definition() {
        let dir = test_dir("forest");
        fs::write(
            dir.join("notes.md"),
            "# Area &&:chimp:metadata:grammar\n- [ ] TODO &grammar\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(forest.chores.len(), 2);
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("&grammar"))
            .unwrap();
        let paths = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"chimp:metadata:grammar"));
        assert!(
            !forest
                .definitions
                .iter()
                .any(|def| def.path == "grammar" && def.is_phony)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_unique_partial_amp_path_back_to_front() {
        let dir = test_dir("partial-resolution");
        fs::write(
            dir.join("notes.md"),
            "# Release &&:Company:Platform:API:Release\n- [ ] TODO &company:api:release matched\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("matched"))
            .unwrap();
        assert!(
            chore
                .definitions
                .iter()
                .any(|id| { forest.definitions[id.0].path == "company:platform:api:release" })
        );
        assert!(!forest.issues.iter().any(|issue| {
            matches!(
                issue.kind,
                CheckIssueKind::UnresolvedAmpPath | CheckIssueKind::AmbiguousAmpPath
            ) && issue.line == Some(2)
        }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_ambiguous_partial_amp_path_matches() {
        let dir = test_dir("partial-resolution-ambiguous");
        fs::write(
            dir.join("notes.md"),
            [
                "# First &&:area:first:release",
                "# Second &&:area:second:release",
                "- [ ] TODO &area:release ambiguous",
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let issue = forest
            .issues
            .iter()
            .find(|issue| issue.kind == CheckIssueKind::AmbiguousAmpPath && issue.line == Some(3))
            .unwrap();
        assert!(issue.message.contains("area:first:release"));
        assert!(issue.message.contains("area:second:release"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_amp_path_matching_preserves_part_order_and_quoted_colons() {
        assert!(amp_path_partial_match(
            "area:`release: one`:task",
            "area:middle:`release: one`:detail:task"
        ));
        assert!(!amp_path_partial_match(
            "`release: one`:area:task",
            "area:middle:`release: one`:detail:task"
        ));
    }

    #[test]
    fn resolves_wikilink_reference_like_colon_amp_path() {
        let dir = test_dir("wikilink-reference");
        fs::write(
            dir.join("notes.md"),
            "# Area &&:a:b\n- [ ] TODO &[[a/b]] linked\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("linked"))
            .unwrap();
        assert!(
            chore
                .definitions
                .iter()
                .any(|id| forest.definitions[id.0].path == "a:b")
        );
        assert!(!forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::UnresolvedAmpPath && issue.line == Some(2)
        }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn quoted_amp_path_colons_are_not_structural_separators() {
        let dir = test_dir("quoted-amp-path");
        fs::write(
            dir.join("notes.md"),
            "# Root &&:root\n## Part &&`part one:two`\n- [ ] TODO &:root:`part one:two` linked\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(
            forest
                .definitions
                .iter()
                .any(|definition| definition.path == "root:`part one:two`")
        );
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("linked"))
            .unwrap();
        let paths = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"root:`part one:two`"));
        assert!(paths.contains(&"root"));
        assert!(
            !forest
                .definitions
                .iter()
                .any(|definition| definition.path == "root:`part one")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn amp_path_depth_ignores_colons_inside_backticks() {
        assert_eq!(amp_path_depth("root:`part one:two`:leaf"), 2);
        assert_eq!(amp_path_tail("root:`part one:two`"), "`part one:two`");
        assert_eq!(amp_path_parent("root:`part one:two`"), Some("root"));
    }

    #[test]
    fn trailing_ampersand_injects_line_amp_paths_into_target_definitions() {
        let dir = test_dir("inverse-injection");
        fs::write(
            dir.join("notes.md"),
            [
                "# Alpha &&:alpha &#3",
                "# Beta &&:beta",
                "# Middle &&:middle",
                "# Target &&:target",
                "# Other &&:other",
                "&alpha &middle&",
                "&middle &beta &target& &other&",
                "- [ ] TODO &target downstream",
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let paths_for = |name: &str| {
            let definition = forest
                .definitions
                .iter()
                .find(|definition| definition.path == name)
                .unwrap();
            definition
                .definitions
                .iter()
                .map(|id| forest.definitions[id.0].path.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(paths_for("middle"), vec!["alpha"]);
        assert_eq!(paths_for("target"), vec!["middle", "beta"]);
        assert_eq!(paths_for("other"), vec!["middle", "beta"]);

        let downstream = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("downstream"))
            .unwrap();
        let related = downstream
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(related.contains(&"target"));
        assert!(related.contains(&"middle"));
        assert!(related.contains(&"beta"));
        assert!(related.contains(&"alpha"));
        assert_eq!(computed_chore_order(&forest, downstream).unwrap().value, 3);

        let injection_line = forest.chores.iter().find(|chore| chore.line == 7).unwrap();
        let injection_related = injection_line
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(!injection_related.contains(&"target"));
        assert!(
            !forest
                .definitions
                .iter()
                .any(|definition| definition.path.ends_with('&'))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wikilink_trailing_ampersand_is_an_inverse_injection_target() {
        let dir = test_dir("wikilink-inverse-injection");
        fs::write(
            dir.join("notes.md"),
            "# Alpha &&:alpha\n# Target &&:target:path\n&alpha &[[TARGET/PATH]]&\n- [ ] &target:path downstream\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let target = forest
            .definitions
            .iter()
            .find(|definition| definition.path == "target:path")
            .unwrap();
        assert_eq!(
            target
                .definitions
                .iter()
                .map(|id| forest.definitions[id.0].path.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha"]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn amp_paths_and_assignees_are_case_insensitive() {
        let dir = test_dir("case-insensitive-amp-paths");
        fs::write(
            dir.join("notes.md"),
            [
                "# Project &&:Project:`Release X`",
                "# Duplicate &&:PROJECT:`RELEASE X`",
                "# Person &&@GeErT",
                "- [ ] &project:`release x` &@GEERT matched",
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(
            forest
                .definitions
                .iter()
                .filter(|definition| definition.path == "project:`release x`")
                .count(),
            1
        );
        assert!(
            forest
                .issues
                .iter()
                .any(|issue| issue.kind == CheckIssueKind::AmbiguousDefinition)
        );
        assert!(
            !forest
                .issues
                .iter()
                .any(|issue| issue.kind == CheckIssueKind::UnresolvedAssignee)
        );
        assert_eq!(
            forest.chores.last().unwrap().assignee.as_deref(),
            Some("geert")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn source_comments_are_scanned_only_when_they_start_with_amp_path() {
        let dir = test_dir("source-comment-prefix");
        fs::write(
            dir.join("notes.rs"),
            "fn main() {}\n  // TODO &ignored false positive\n  // &accepted TODO and &additional\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(forest.chores.len(), 1);
        assert!(forest.chores[0].text.contains("&accepted"));
        let paths = forest.chores[0]
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"accepted"));
        assert!(paths.contains(&"additional"));
        assert!(
            !forest
                .definitions
                .iter()
                .any(|definition| definition.path == "ignored")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trailing_definition_derives_folder_and_file_definitions() {
        let dir = test_dir("filesystem-definitions");
        fs::write(dir.join("&.md"), "&&:Knowledge&\n").unwrap();
        fs::create_dir_all(dir.join("Projects")).unwrap();
        fs::write(
            dir.join("Projects").join("My Notes.md"),
            "- [ ] TODO derived chore\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("Projects").join("Stopped")).unwrap();
        fs::write(
            dir.join("Projects").join("Stopped").join("&.md"),
            "&&:manual\n",
        )
        .unwrap();
        fs::write(
            dir.join("Projects").join("Stopped").join("hidden.md"),
            "- [ ] TODO stopped chore\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("Projects").join("Stopped").join("Restarted")).unwrap();
        fs::write(
            dir.join("Projects")
                .join("Stopped")
                .join("Restarted")
                .join("&.md"),
            "&&:Fresh&\n",
        )
        .unwrap();
        fs::write(
            dir.join("Projects")
                .join("Stopped")
                .join("Restarted")
                .join("again.md"),
            "- [ ] TODO restarted chore\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let paths = forest
            .definitions
            .iter()
            .map(|definition| definition.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"knowledge:projects"));
        assert!(paths.contains(&"knowledge:projects:`my notes`"));
        assert!(!paths.contains(&"knowledge:projects:stopped:hidden"));
        assert!(paths.contains(&"fresh:again"));

        let derived = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("derived chore"))
            .unwrap();
        let related = derived
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        assert!(related.contains(&"knowledge:projects:`my notes`"));
        assert!(related.contains(&"knowledge:projects"));
        assert!(related.contains(&"knowledge"));

        let restarted = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("restarted chore"))
            .unwrap();
        assert!(
            restarted
                .definitions
                .iter()
                .any(|id| forest.definitions[id.0].path == "fresh:again")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_phony_definition_for_unresolved_reference() {
        let dir = test_dir("phony");
        fs::write(dir.join("notes.md"), "- [ ] TODO &missing\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(
            forest
                .definitions
                .iter()
                .any(|def| def.path == "missing" && def.is_phony)
        );
        assert!(
            forest
                .issues
                .iter()
                .any(|issue| issue.kind == CheckIssueKind::UnresolvedAmpPath)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_relative_definition_without_parent() {
        let dir = test_dir("relative");
        fs::write(dir.join("notes.md"), "## Orphan &&orphan\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::RelativeDefinitionWithoutParent && issue.line == Some(1)
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_duplicate_definitions_without_one_exclusive_prime() {
        let dir = test_dir("duplicate-definitions");
        fs::write(dir.join("a.md"), "# Work &&:work\n").unwrap();
        fs::write(dir.join("b.md"), "# Work too &&:work\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(
            forest
                .issues
                .iter()
                .filter(|issue| issue.kind == CheckIssueKind::AmbiguousDefinition)
                .count(),
            2
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exclusive_definition_selects_prime_and_resolves_ambiguity() {
        let dir = test_dir("exclusive-definition");
        fs::write(dir.join("a.md"), "# Work &&:work &#1\n").unwrap();
        fs::write(dir.join("b.md"), "# Work prime &&^:work &#9\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(
            !forest
                .issues
                .iter()
                .any(|issue| issue.kind == CheckIssueKind::AmbiguousDefinition)
        );
        let definition = forest
            .definitions
            .iter()
            .find(|definition| definition.path == "work")
            .unwrap();
        assert!(definition.exclusive);
        assert_eq!(definition.order.map(|order| order.value), Some(9));
        assert!(
            forest.files[definition.file.unwrap().0]
                .path
                .ends_with("b.md")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn assignee_definitions_validate_chore_assignments() {
        let dir = test_dir("assignee-definitions");
        fs::write(
            dir.join("notes.md"),
            "# Alice &&@alice\n- [ ] &@alice assigned\n- [ ] &@bob missing\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let alice = forest
            .definitions
            .iter()
            .find(|definition| definition.path == "alice")
            .unwrap();
        assert!(alice.is_assignee);
        assert!(!forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::UnresolvedAssignee && issue.line == Some(2)
        }));
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::UnresolvedAssignee && issue.line == Some(3)
        }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bare_assignees_resolve_only_against_assignee_definitions() {
        let dir = test_dir("bare-assignees");
        fs::write(
            dir.join("notes.md"),
            "# Alice &&@alice\n# Topic &&:bob\n- [ ] assigned @Alice\n- [ ] not assigned @bob\n- [ ] unknown @carol\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let chore = |text: &str| {
            forest
                .chores
                .iter()
                .find(|chore| chore.text.contains(text))
                .unwrap()
        };
        assert_eq!(chore("assigned @Alice").assignee.as_deref(), Some("alice"));
        assert!(chore("not assigned @bob").assignee.is_none());
        assert!(chore("unknown @carol").assignee.is_none());
        assert!(!forest.issues.iter().any(|issue| matches!(
            issue.kind,
            CheckIssueKind::UnresolvedAssignee | CheckIssueKind::AmbiguousAssignee
        )));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dates_in_folder_and_file_names_reach_chores_and_definitions() {
        let dir = test_dir("filesystem-dates");
        let dated = dir.join("planning-2026-08-06");
        fs::create_dir_all(&dated).unwrap();
        fs::write(
            dated.join("followup-20260907.md"),
            "# Project &&:project\n- [ ] dated task\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(
            forest
                .chores
                .iter()
                .find(|chore| chore.text.contains("dated task"))
                .and_then(|chore| chore.date.as_deref()),
            Some("20260806")
        );
        assert_eq!(
            forest
                .definitions
                .iter()
                .find(|definition| definition.path == "project")
                .and_then(|definition| definition.date.as_deref()),
            Some("20260806")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mixed_case_lowercase_filename_creates_file_level_todo() {
        let dir = test_dir("filename-todo");
        fs::write(dir.join("&.md"), "&&:knowledge&\n").unwrap();
        fs::write(dir.join("someTask.md"), "").unwrap();
        fs::write(dir.join("Some Task.md"), "").unwrap();
        fs::write(dir.join("lowercase.md"), "").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let filename_chores = forest
            .chores
            .iter()
            .filter(|chore| chore.text.contains("someTask") || chore.text.contains("Some Task"))
            .collect::<Vec<_>>();
        assert_eq!(filename_chores.len(), 2);
        let todo = filename_chores
            .iter()
            .find(|chore| chore.text.contains("someTask"))
            .unwrap();
        let done = filename_chores
            .iter()
            .find(|chore| chore.text.contains("Some Task"))
            .unwrap();
        assert_eq!(todo.text, "- [ ] someTask");
        assert_eq!(todo.status, Some(Status::Todo));
        assert!(
            todo.definitions
                .iter()
                .any(|id| { forest.definitions[id.0].path == "knowledge:sometask" })
        );
        assert_eq!(done.text, "- [x] Some Task");
        assert_eq!(done.status, Some(Status::Done));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn filename_status_tracks_case_transition() {
        assert_eq!(filename_status("someTask"), Some(Status::Todo));
        assert_eq!(filename_status("aB"), Some(Status::Todo));
        assert_eq!(filename_status("SomeTask"), Some(Status::Done));
        assert_eq!(filename_status("Some Task"), Some(Status::Done));
        assert_eq!(filename_status("some task"), None);
        assert_eq!(filename_status("2026someTask"), None);
    }

    #[test]
    fn filename_status_creates_chores_only_inside_filesystem_definition_cascades() {
        let dir = test_dir("filename-todo-scope");
        fs::write(dir.join("outsideTask.md"), "").unwrap();
        fs::create_dir_all(dir.join("enabled")).unwrap();
        fs::write(dir.join("enabled").join("&.md"), "&&:enabled&\n").unwrap();
        fs::write(dir.join("enabled").join("insideTask.md"), "").unwrap();
        fs::create_dir_all(dir.join("enabled").join("stopped")).unwrap();
        fs::write(
            dir.join("enabled").join("stopped").join("&.md"),
            "&&:stopped\n",
        )
        .unwrap();
        fs::write(
            dir.join("enabled").join("stopped").join("stoppedTask.md"),
            "",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let filenames = forest
            .chores
            .iter()
            .map(|chore| chore.text.as_str())
            .collect::<Vec<_>>();
        assert!(filenames.contains(&"- [ ] insideTask"));
        assert!(!filenames.contains(&"- [ ] outsideTask"));
        assert!(!filenames.contains(&"- [ ] stoppedTask"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_amp_paths_are_omitted_and_reported() {
        let dir = test_dir("empty-amp-path");
        fs::write(dir.join("notes.md"), "- [ ] keep this chore & &&\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(forest.chores.len(), 1);
        assert!(forest.definitions.is_empty());
        let issues = forest
            .issues
            .iter()
            .filter(|issue| issue.kind == CheckIssueKind::EmptyAmpPath)
            .collect::<Vec<_>>();
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|issue| issue.line == Some(1)));
        assert!(issues.iter().all(|issue| issue.message.contains("omitted")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_markdown_parsing_issues() {
        let dir = test_dir("markdown-issues");
        fs::write(
            dir.join("notes.md"),
            "Text `unterminated\n```rust\nTODO &hidden\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::MarkdownParsing && issue.message.contains("inline code")
        }));
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::MarkdownParsing
                && issue.message.contains("fenced code block")
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_wbs_metadata_without_same_line_definition() {
        let dir = test_dir("wbs-missing-definition");
        fs::write(
            dir.join("notes.md"),
            "# Root &&:root\n- [ ] &?project Define the work\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::WbsWithoutDefinition && issue.line == Some(2)
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn attaches_wbs_metadata_to_same_line_definition() {
        let dir = test_dir("wbs-definition");
        fs::write(dir.join("notes.md"), "- [ ] &&:root:project &?project\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let definition = forest
            .definitions
            .iter()
            .find(|definition| definition.path == "root:project")
            .unwrap();
        assert_eq!(definition.wbs, vec!["project"]);
        assert!(
            !forest
                .issues
                .iter()
                .any(|issue| issue.kind == CheckIssueKind::WbsWithoutDefinition)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn chore_references_existing_ancestor_definitions() {
        let dir = test_dir("ancestors");
        fs::write(
            dir.join("notes.md"),
            "# Root &&:a &#9\n## Middle &&:a:b &#4\n- [ ] TODO &a:b:c leaf\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("leaf"))
            .unwrap();
        let paths = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();

        assert!(paths.contains(&"a"));
        assert!(paths.contains(&"a:b"));
        assert!(paths.contains(&"a:b:c"));
        assert_eq!(computed_chore_order(&forest, chore).unwrap().value, 4);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn conflicting_exclusive_orders_are_reported() {
        let dir = test_dir("exclusive-conflict");
        fs::write(
            dir.join("notes.md"),
            "# Root &&:a &^#9\n## Middle &&:a:b &^#4\n- [ ] TODO &a:b:c leaf\n",
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![dir.clone()])).unwrap();
        let chore = forest
            .chores
            .iter()
            .find(|chore| chore.text.contains("leaf"))
            .unwrap();
        let order = computed_chore_order(&forest, chore).unwrap();

        assert_eq!(order.value, 4);
        assert!(order.exclusive);
        assert!(order.conflict);
        assert!(forest.issues.iter().any(|issue| {
            issue.kind == CheckIssueKind::ConflictingExclusiveOrder && issue.line == Some(3)
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_original_bytes_exactly() {
        let dir = test_dir("roundtrip");
        let source = dir.join("notes.md");
        let dest = dir.join("copy.md");
        let content = b"# Title\r\n- [ ] TODO &roundtrip\r\n";
        fs::write(&source, content).unwrap();

        let files = load_files(&Config::from_roots(vec![dir.clone()])).unwrap();
        let file = files.iter().find(|file| file.path == source).unwrap();
        write_file_exact(file, &dest).unwrap();

        assert_eq!(fs::read(dest).unwrap(), content);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exports_filtered_files_and_strips_amp_metadata() {
        let root = test_dir("export-root");
        let dest = test_dir("export-dest");
        fs::remove_dir_all(&dest).unwrap();
        fs::write(root.join("notes.md"), "- [ ] &keep:task Do the thing\n").unwrap();
        fs::write(
            root.join("code.rs"),
            "fn main() {}\n  // WIP &keep:code build it\n",
        )
        .unwrap();
        fs::write(root.join("other.md"), "- [x] &skip done\n").unwrap();

        let forest = build_forest(&Config::from_roots(vec![root.clone()])).unwrap();
        let summary = export_forest(
            &forest,
            &dest,
            &ExportOptions {
                include_amp_metadata: false,
                status: Some(Status::Todo),
                amp_tags: vec!["keep:task".to_string()],
                extensions: vec!["md".to_string()],
            },
        )
        .unwrap();

        assert_eq!(summary.files_written, 1);
        let exported = fs::read_to_string(dest.join("notes.md")).unwrap();
        assert_eq!(exported, "- [ ] Do the thing\n");
        assert!(!dest.join("code.rs").exists());
        assert!(!dest.join("other.md").exists());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(dest).unwrap();
    }

    #[test]
    fn rejects_export_destination_inside_grove() {
        let root = test_dir("export-safe");
        fs::write(root.join("notes.md"), "- [ ] &keep\n").unwrap();
        let forest = build_forest(&Config::from_roots(vec![root.clone()])).unwrap();
        let result = export_forest(&forest, root.join("out"), &ExportOptions::default());

        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn markdown_ignores_inline_and_block_code_or_formula_metadata() {
        let root = test_dir("markdown-code");
        fs::write(
            root.join("notes.md"),
            [
                "# Root &&:root",
                "Inline `TODO &inline_code` and $WIP &inline_math$ text.",
                "```rust",
                "// TODO &block_code",
                "```",
                "$$",
                "TODO &block_math",
                "$$",
                "- [ ] &real_task Keep this",
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let forest = build_forest(&Config::from_roots(vec![root.clone()])).unwrap();
        let chore_texts = forest
            .chores
            .iter()
            .map(|chore| chore.text.as_str())
            .collect::<Vec<_>>();

        assert!(chore_texts.iter().any(|text| text.contains("&real_task")));
        assert!(!chore_texts.iter().any(|text| text.contains("&inline_code")));
        assert!(!chore_texts.iter().any(|text| text.contains("&inline_math")));
        assert!(!chore_texts.iter().any(|text| text.contains("&block_code")));
        assert!(!chore_texts.iter().any(|text| text.contains("&block_math")));

        fs::remove_dir_all(root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chimp-{name}-{unique}"));
        fs::create_dir(&dir).unwrap();
        dir
    }
}
