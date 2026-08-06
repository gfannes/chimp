pub mod naft;
mod parse;
mod scan;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub use parse::{Metadata, extract_metadata};
pub use scan::{load_files, write_file_exact};

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
    pub text: String,
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
    pub file: Option<FileId>,
    pub line: Option<usize>,
    pub date: Option<String>,
    pub order: Option<OrderMetadata>,
    pub assignee: Option<String>,
    pub wbs: Vec<String>,
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
    pub wbs: Vec<String>,
    pub definitions: Vec<DefinitionId>,
}

#[derive(Debug, Clone)]
pub struct Forest {
    pub files: Vec<SourceFile>,
    pub definitions: Vec<Definition>,
    pub chores: Vec<Chore>,
    pub issues: Vec<CheckIssue>,
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
    RelativeDefinitionWithoutParent,
    WbsWithoutDefinition,
    MarkdownParsing,
    ConflictingExclusiveOrder,
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
            path == tag
                || path.ends_with(&format!(":{tag}"))
                || path.split(':').next_back().is_some_and(|tail| tail == tag)
        })
    })
}

#[derive(Debug, Clone)]
struct ParsedLine {
    file: FileId,
    line: usize,
    column: usize,
    text: String,
    metadata: Metadata,
    inherited_refs: Vec<String>,
    is_chore: bool,
}

pub fn build_forest(config: &Config) -> Result<Forest> {
    let files = load_files(config)?;
    let mut builder = ForestBuilder::new(files);
    builder.parse_files();
    Ok(builder.finish())
}

struct ForestBuilder {
    files: Vec<SourceFile>,
    definitions: Vec<Definition>,
    definition_by_path: HashMap<String, DefinitionId>,
    parsed_lines: Vec<ParsedLine>,
    folder_metadata: HashMap<(usize, PathBuf), Metadata>,
    file_metadata: HashMap<FileId, Metadata>,
    issues: Vec<CheckIssue>,
}

impl ForestBuilder {
    fn new(files: Vec<SourceFile>) -> Self {
        Self {
            files,
            definitions: Vec::new(),
            definition_by_path: HashMap::new(),
            parsed_lines: Vec::new(),
            folder_metadata: HashMap::new(),
            file_metadata: HashMap::new(),
            issues: Vec::new(),
        }
    }

    fn parse_files(&mut self) {
        for index in 0..self.files.len() {
            self.parse_file(FileId(index));
        }
    }

    fn parse_file(&mut self, file_id: FileId) {
        let file = &self.files[file_id.0];
        let path = file.path.clone();
        let grove = file.grove;
        let text = file.text.clone();
        let is_markdown = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        let is_folder_metadata = path.file_name().and_then(|name| name.to_str()) == Some("&.md");
        let mut heading_stack: Vec<(usize, Metadata)> = Vec::new();
        let mut bullet_stack: Vec<(usize, Metadata)> = Vec::new();
        let mut folder_md = Metadata::default();
        let mut markdown_state = parse::MarkdownState::default();

        let lines = text
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line.to_string()))
            .collect::<Vec<_>>();

        for (line_no, raw_line) in lines {
            let Some(content) = parse::content_line(&raw_line, is_markdown, &path) else {
                continue;
            };
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
            let metadata = extract_metadata(metadata_source);
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
                self.upsert_definition(&path, false, Some(file_id), Some(line_no), &metadata);
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
                    text: content.text.to_string(),
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

        if is_folder_metadata && !folder_md.is_empty() {
            if let Some(parent) = path.parent() {
                self.folder_metadata
                    .insert((grove, parent.to_path_buf()), folder_md);
            }
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

    fn upsert_definition(
        &mut self,
        path: &str,
        is_phony: bool,
        file: Option<FileId>,
        line: Option<usize>,
        md: &Metadata,
    ) -> DefinitionId {
        if let Some(id) = self.definition_by_path.get(path).copied() {
            let def = &mut self.definitions[id.0];
            def.is_phony &= is_phony;
            def.file = def.file.or(file);
            def.line = def.line.or(line);
            def.date = def.date.clone().or_else(|| md.date.clone());
            def.order = def.order.or(md.order);
            def.assignee = def.assignee.clone().or_else(|| md.assignee.clone());
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
            file,
            line,
            date: md.date.clone(),
            order: md.order,
            assignee: md.assignee.clone(),
            wbs: md.wbs.clone(),
        });
        id
    }

    fn finish(mut self) -> Forest {
        let mut chores = Vec::new();
        let parsed_lines = std::mem::take(&mut self.parsed_lines);
        for line in parsed_lines.into_iter().filter(|line| line.is_chore) {
            let mut inherited_amp_paths = self.folder_context(line.file);
            if let Some(md) = self.file_metadata.get(&line.file) {
                inherited_amp_paths.extend(md.references.iter().cloned());
                inherited_amp_paths.extend(md.definitions.iter().cloned());
            }
            inherited_amp_paths.extend(line.inherited_refs);

            let mut ids = Vec::new();
            let mut seen = HashSet::new();
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

            let chore = Chore {
                file: line.file,
                line: line.line,
                column: line.column,
                text: line.text,
                status: line.metadata.status,
                date: line.metadata.date,
                order: line.metadata.order,
                assignee: line.metadata.assignee,
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

        Forest {
            files: self.files,
            definitions: self.definitions,
            chores,
            issues: self.issues,
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
        let matches = self
            .definitions
            .iter()
            .filter(|def| {
                def.path == reference
                    || def.path.ends_with(&suffix)
                    || def
                        .path
                        .split(':')
                        .next_back()
                        .is_some_and(|tail| tail == reference)
            })
            .map(|def| def.id)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [id] => *id,
            [] => {
                self.issues.push(CheckIssue {
                    kind: CheckIssueKind::UnresolvedAmpPath,
                    file: Some(file),
                    line: Some(line),
                    message: format!("AmpPath {amp} could not be resolved to a Definition"),
                });
                self.upsert_definition(&reference, true, Some(file), Some(line), md)
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
                self.upsert_definition(&reference, true, Some(file), Some(line), md)
            }
        }
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
        while let Some((parent, _)) = current.rsplit_once(':') {
            if parent.is_empty() {
                break;
            }
            if let Some(id) = builder.definition_by_path.get(parent).copied() {
                if seen.insert(id) {
                    ids.push(id);
                }
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
        builder.upsert_definition(&path, false, Some(file), Some(line), metadata)
    } else {
        builder.resolve_reference(amp, metadata, file, line)
    };
    if seen.insert(id) {
        ids.push(id);
    }
}

fn resolve_definition_path(amp: &str, parent: Option<&String>) -> String {
    let raw = amp.trim_start_matches('&');
    if raw.starts_with(':') {
        raw.trim_start_matches(':').trim_matches(':').to_string()
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
    amp.trim_start_matches('&').starts_with(':')
}

fn normalize_amp_path(amp: &str) -> String {
    amp.trim_start_matches('&')
        .trim_start_matches(':')
        .trim_matches(':')
        .to_string()
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
