use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chimp::{
    CheckIssue, Chore, Config, ExportOptions, Forest, GroveConfig, Status, build_forest,
    export_forest,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "scan" => {
            let config = Config::from_groves(parse_path_groves(args)?);
            let forest = build_forest(&config)?;
            println!(
                "files={} definitions={} chores={}",
                forest.files.len(),
                forest.definitions.len(),
                forest.chores.len()
            );
        }
        "chores" => {
            let mut extra_roots = Vec::new();
            let mut status_filter = None;
            let mut assignee_filters = Vec::new();
            let mut query_terms = Vec::new();

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-s" | "--status" => {
                        let value = args.next().ok_or("--status requires a value")?;
                        status_filter = Some(parse_status(&value)?);
                    }
                    "-a" | "--assignee" => {
                        assignee_filters.push(args.next().ok_or("--assignee requires a value")?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ if arg.starts_with('@') && arg.len() > 1 => {
                        assignee_filters.push(arg.trim_start_matches('@').to_string());
                    }
                    _ => query_terms.push(arg),
                }
            }

            let forest = build_forest(&Config::from_groves(default_groves_with_extra(
                extra_roots,
            )?))?;
            print_chore_search_results(
                &forest,
                status_filter,
                &assignee_filters,
                &query_terms,
                true,
            );
        }
        "wbs" => {
            let mut extra_roots = Vec::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            let forest = build_forest(&Config::from_groves(default_groves_with_extra(
                extra_roots,
            )?))?;
            print_wbs_results(&forest, true);
        }
        "groves" => {
            let mut extra_roots = Vec::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            print_groves(&default_groves_with_extra(extra_roots)?);
        }
        "check" => {
            let mut extra_roots = Vec::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            let forest = build_forest(&Config::from_groves(default_groves_with_extra(
                extra_roots,
            )?))?;
            print_check_issues(&forest);
        }
        "export" => {
            let mut destination = None;
            let mut roots = Vec::new();
            let mut options = ExportOptions {
                include_amp_metadata: true,
                ..ExportOptions::default()
            };

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--include-amp" => options.include_amp_metadata = true,
                    "--strip-amp" => options.include_amp_metadata = false,
                    "-s" | "--status" => {
                        let value = args.next().ok_or("--status requires a value")?;
                        options.status = Some(parse_status(&value)?);
                    }
                    "--amp" => {
                        options
                            .amp_tags
                            .push(args.next().ok_or("--amp requires a value")?);
                    }
                    "--ext" | "--extension" => {
                        options
                            .extensions
                            .push(args.next().ok_or("--ext requires a value")?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => {
                        if destination.is_none() {
                            destination = Some(PathBuf::from(arg));
                        } else {
                            roots.push(PathBuf::from(arg));
                        }
                    }
                }
            }

            let destination = destination.ok_or("export requires a destination folder")?;
            let forest = build_forest(&Config::from_groves(default_groves(roots)?))?;
            let summary = export_forest(&forest, destination, &options)?;
            println!("files_written={}", summary.files_written);
        }
        "help" | "--help" | "-h" => print_help(),
        _ => return Err(format!("unknown command: {command}").into()),
    }

    Ok(())
}

fn parse_path_groves(args: impl Iterator<Item = String>) -> Result<Vec<GroveConfig>> {
    let mut roots = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            return Err(format!("unexpected argument: {arg}").into());
        }
        roots.push(PathBuf::from(arg));
    }
    default_groves(roots)
}

fn default_groves(roots: Vec<PathBuf>) -> Result<Vec<GroveConfig>> {
    if !roots.is_empty() {
        return Ok(roots.into_iter().map(GroveConfig::from_root).collect());
    }
    if let Some(config_groves) = read_config_groves_from_default_locations()? {
        return Ok(config_groves);
    }
    Ok(vec![GroveConfig::from_root(std::env::current_dir()?)])
}

fn default_groves_with_extra(extra_roots: Vec<PathBuf>) -> Result<Vec<GroveConfig>> {
    let mut groves = read_config_groves_from_default_locations()?.unwrap_or_default();
    groves.extend(extra_roots.into_iter().map(GroveConfig::from_root));
    if groves.is_empty() {
        groves.push(GroveConfig::from_root(std::env::current_dir()?));
    }
    Ok(groves)
}

fn read_config_groves_from_default_locations() -> Result<Option<Vec<GroveConfig>>> {
    let mut groves = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join(".config/chimp/config.toml");
        groves.extend(read_config_groves(&path)?);
    }
    groves.extend(read_config_groves(Path::new("chimp.toml"))?);
    if groves.is_empty() {
        Ok(None)
    } else {
        Ok(Some(groves))
    }
}

fn read_config_groves(path: &Path) -> Result<Vec<GroveConfig>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let text = std::fs::read_to_string(path)?;
    Ok(parse_config_groves(base, &text))
}

fn config_root_path(base: &Path, root: PathBuf) -> PathBuf {
    if root.is_absolute() {
        root
    } else {
        base.join(root)
    }
}

#[derive(Debug, Default)]
struct GroveDraft {
    root: Option<PathBuf>,
    extensions: Vec<String>,
    max_filesize: Option<u64>,
}

impl GroveDraft {
    fn finish(self, base: &Path) -> Option<GroveConfig> {
        Some(GroveConfig {
            root: config_root_path(base, self.root?),
            extensions: self.extensions,
            max_filesize: self.max_filesize,
        })
    }
}

fn parse_config_groves(base: &Path, text: &str) -> Vec<GroveConfig> {
    let mut groves = Vec::new();
    let mut current = None::<GroveDraft>;

    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line == "[[grove]]" || line == "[[groves]]" {
            if let Some(draft) = current.take().and_then(|draft| draft.finish(base)) {
                groves.push(draft);
            }
            current = Some(GroveDraft::default());
            continue;
        }

        if let Some(draft) = current.as_mut() {
            apply_grove_config_line(draft, line);
        } else {
            groves.extend(
                parse_root_config_line(line)
                    .into_iter()
                    .map(|root| GroveConfig::from_root(config_root_path(base, root))),
            );
        }
    }

    if let Some(draft) = current.and_then(|draft| draft.finish(base)) {
        groves.push(draft);
    }

    groves
}

fn apply_grove_config_line(draft: &mut GroveDraft, line: &str) {
    if let Some(value) = config_value(line, "path").or_else(|| config_value(line, "root")) {
        draft.root = quoted_value(value).map(PathBuf::from);
    } else if let Some(value) = config_value(line, "extensions") {
        draft.extensions = parse_string_array(value);
    } else if let Some(value) =
        config_value(line, "max_filesize").or_else(|| config_value(line, "max_file_size"))
    {
        draft.max_filesize = value.trim().parse::<u64>().ok();
    }
}

fn config_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key)
        .and_then(|line| line.trim_start().strip_prefix('='))
        .map(str::trim)
}

fn parse_root_config_line(line: &str) -> Vec<PathBuf> {
    if line.starts_with('#') || line.is_empty() {
        return Vec::new();
    }
    if let Some(value) = line
        .strip_prefix("root")
        .and_then(|line| line.trim_start().strip_prefix('='))
    {
        return quoted_value(value).map(PathBuf::from).into_iter().collect();
    }
    if let Some(value) = line
        .strip_prefix("roots")
        .and_then(|line| line.trim_start().strip_prefix('='))
    {
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        else {
            return Vec::new();
        };
        return value
            .split(',')
            .filter_map(quoted_value)
            .map(PathBuf::from)
            .collect();
    }
    Vec::new()
}

fn parse_string_array(value: &str) -> Vec<String> {
    let value = value.trim();
    let Some(value) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Vec::new();
    };
    value
        .split(',')
        .filter_map(quoted_value)
        .map(|value| value.trim_start_matches('.').to_string())
        .collect()
}

fn quoted_value(value: &str) -> Option<&str> {
    value
        .trim()
        .trim_matches(',')
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
}

fn parse_status(value: &str) -> Result<Status> {
    match value {
        "TODO" | "todo" => Ok(Status::Todo),
        "DONE" | "done" => Ok(Status::Done),
        "WIP" | "wip" => Ok(Status::Wip),
        _ => Err("status must be TODO, DONE, or WIP".into()),
    }
}

fn print_groves(groves: &[GroveConfig]) {
    for (index, grove) in groves.iter().enumerate() {
        let extensions = if grove.extensions.is_empty() {
            "default".to_string()
        } else {
            grove.extensions.join(",")
        };
        let max_filesize = grove
            .max_filesize
            .map(|size| size.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}: path={} extensions={} max_filesize={}",
            index + 1,
            display_grove_path(&grove.root).display(),
            extensions,
            max_filesize
        );
    }
}

fn display_grove_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn print_check_issues(forest: &Forest) {
    for issue in &forest.issues {
        println!("{}", format_check_issue(forest, issue));
    }
    println!("issues={}", forest.issues.len());
}

fn format_check_issue(forest: &Forest, issue: &CheckIssue) -> String {
    let location = match (issue.file, issue.line) {
        (Some(file), Some(line)) => format!("{}:{line}", forest.files[file.0].path.display()),
        (Some(file), None) => forest.files[file.0].path.display().to_string(),
        (None, Some(line)) => format!("<unknown>:{line}"),
        (None, None) => "<unknown>".to_string(),
    };
    format!("{location}: {:?}: {}", issue.kind, issue.message)
}

fn chore_matches(
    forest: &Forest,
    chore: &Chore,
    status_filter: Option<Status>,
    assignee_filters: &[String],
    query_terms: &[String],
) -> bool {
    if status_filter.is_some_and(|status| chore.status != Some(status)) {
        return false;
    }
    if !assignee_filters.is_empty()
        && !assignee_filters.iter().any(|assignee| {
            chore.assignee.as_deref() == Some(assignee.as_str())
                || chore.definitions.iter().any(|id| {
                    forest.definitions[id.0].assignee.as_deref() == Some(assignee.as_str())
                })
        })
    {
        return false;
    }
    query_terms
        .iter()
        .all(|term| searchable_text_contains(forest, chore, term))
}

fn searchable_text_contains(forest: &Forest, chore: &Chore, term: &str) -> bool {
    let term = term.to_ascii_lowercase();
    if chore.text.to_ascii_lowercase().contains(&term) {
        return true;
    }
    let file = &forest.files[chore.file.0];
    if file
        .path
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains(&term)
    {
        return true;
    }
    chore.definitions.iter().any(|id| {
        let definition = &forest.definitions[id.0];
        definition.path.to_ascii_lowercase().contains(&term)
            || definition
                .assignee
                .as_deref()
                .is_some_and(|assignee| assignee.to_ascii_lowercase().contains(&term))
    })
}

fn max_definition_order(forest: &Forest, chore: &Chore) -> Option<u32> {
    chore
        .definitions
        .iter()
        .filter_map(|id| forest.definitions[id.0].order)
        .max()
}

fn chore_has_wbs(forest: &Forest, chore: &Chore) -> bool {
    !chore.wbs.is_empty()
        || chore
            .definitions
            .iter()
            .any(|id| !forest.definitions[id.0].wbs.is_empty())
}

fn print_wbs_results(forest: &Forest, color: bool) {
    print_chore_results(forest, color, |forest, chore| chore_has_wbs(forest, chore));
}

fn print_chore_search_results(
    forest: &Forest,
    status_filter: Option<Status>,
    assignee_filters: &[String],
    query_terms: &[String],
    color: bool,
) {
    print_chore_results(forest, color, |forest, chore| {
        chore_matches(forest, chore, status_filter, assignee_filters, query_terms)
    });
}

fn print_chore_results(forest: &Forest, color: bool, matches: impl Fn(&Forest, &Chore) -> bool) {
    let mut by_file: BTreeMap<usize, Vec<&Chore>> = BTreeMap::new();
    for chore in forest.chores.iter().filter(|chore| matches(forest, chore)) {
        by_file.entry(chore.file.0).or_default().push(chore);
    }

    let mut file_groups = by_file.into_iter().collect::<Vec<_>>();
    file_groups.sort_by(|(left_id, left_chores), (right_id, right_chores)| {
        let left_order = left_chores
            .iter()
            .filter_map(|chore| max_definition_order(forest, chore))
            .max();
        let right_order = right_chores
            .iter()
            .filter_map(|chore| max_definition_order(forest, chore))
            .max();
        right_order.cmp(&left_order).then_with(|| {
            forest.files[*left_id]
                .path
                .cmp(&forest.files[*right_id].path)
        })
    });

    for (file_id, mut chores) in file_groups {
        chores.sort_by(|left, right| {
            max_definition_order(forest, right)
                .cmp(&max_definition_order(forest, left))
                .then_with(|| left.line.cmp(&right.line))
                .then_with(|| left.column.cmp(&right.column))
        });

        println!("{}", forest.files[file_id].path.display());
        let mut current_order = None;
        for chore in chores {
            let order = max_definition_order(forest, chore);
            if current_order != Some(order) {
                current_order = Some(order);
                println!("  {}", colored_order(order, color));
            }
            let defs = chore
                .definitions
                .iter()
                .map(|id| forest.definitions[id.0].path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "    {}:{} [{}] {}",
                chore.line,
                chore.column,
                defs,
                chore.text.trim()
            );
        }
    }
}

fn colored_order(order: Option<u32>, color: bool) -> String {
    let label = order
        .map(|order| format!("order={order}"))
        .unwrap_or_else(|| "order=-".to_string());
    if !color {
        return label;
    }
    match order {
        Some(value) => format!("\x1b[{}m{label}\x1b[0m", order_color(value)),
        None => format!("\x1b[90m{label}\x1b[0m"),
    }
}

fn order_color(order: u32) -> u8 {
    match order % 6 {
        0 => 36,
        1 => 32,
        2 => 33,
        3 => 35,
        4 => 34,
        _ => 31,
    }
}

fn print_help() {
    println!(
        "chimp\n\nUSAGE:\n  chimp scan [PATH...]\n  chimp groves [-C PATH]\n  chimp check [-C PATH]\n  chimp wbs [-C PATH]\n  chimp chores [-C PATH] [--status TODO|DONE|WIP] [--assignee NAME|@NAME] [QUERY...]\n  chimp export DEST [--include-amp|--strip-amp] [--status TODO|DONE|WIP] [--amp TAG] [--ext EXT] [PATH...]\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chimp::{Chore, Definition, DefinitionId, FileId, SourceFile};

    #[test]
    fn parses_config_roots() {
        assert_eq!(
            parse_root_config_line(r#"root = "projects/one""#),
            vec![PathBuf::from("projects/one")]
        );
        assert_eq!(
            parse_root_config_line(r#"roots = ["a", "b"]"#),
            vec![PathBuf::from("a"), PathBuf::from("b")]
        );
    }

    #[test]
    fn parses_per_grove_config() {
        let groves = parse_config_groves(
            Path::new("/tmp/config"),
            r#"
[[grove]]
path = "docs"
extensions = ["md", ".txt"]
max_filesize = 1024

[[grove]]
root = "/abs/src"
extensions = ["rs"]
"#,
        );

        assert_eq!(groves.len(), 2);
        assert_eq!(groves[0].root, PathBuf::from("/tmp/config/docs"));
        assert_eq!(groves[0].extensions, vec!["md", "txt"]);
        assert_eq!(groves[0].max_filesize, Some(1024));
        assert_eq!(groves[1].root, PathBuf::from("/abs/src"));
        assert_eq!(groves[1].extensions, vec!["rs"]);
        assert_eq!(groves[1].max_filesize, None);
    }

    #[test]
    fn query_and_assignee_match_chore_data() {
        let forest = sample_forest();
        let chore = &forest.chores[0];

        assert!(chore_matches(
            &forest,
            chore,
            Some(Status::Todo),
            &["geert".to_string()],
            &["parser".to_string()]
        ));
        assert!(!chore_matches(
            &forest,
            chore,
            Some(Status::Todo),
            &["someone".to_string()],
            &["parser".to_string()]
        ));
        assert!(!chore_matches(
            &forest,
            chore,
            Some(Status::Done),
            &[],
            &["parser".to_string()]
        ));
    }

    #[test]
    fn max_order_comes_from_resolved_definitions() {
        let forest = sample_forest();
        assert_eq!(max_definition_order(&forest, &forest.chores[0]), Some(12));
    }

    #[test]
    fn wbs_match_uses_chore_or_definition_metadata() {
        let forest = sample_forest();
        assert!(chore_has_wbs(&forest, &forest.chores[0]));
    }

    #[test]
    fn order_labels_are_color_coded() {
        assert_eq!(colored_order(Some(1), false), "order=1");
        assert_eq!(colored_order(None, false), "order=-");
        assert_eq!(colored_order(Some(1), true), "\x1b[32morder=1\x1b[0m");
        assert_eq!(colored_order(None, true), "\x1b[90morder=-\x1b[0m");
    }

    #[test]
    fn formats_check_issue_with_location() {
        let mut forest = sample_forest();
        forest.issues.push(CheckIssue {
            kind: chimp::CheckIssueKind::UnresolvedAmpPath,
            file: Some(FileId(0)),
            line: Some(2),
            message: "AmpPath &missing could not be resolved to a Definition".to_string(),
        });

        assert_eq!(
            format_check_issue(&forest, &forest.issues[0]),
            "/tmp/grove/notes.md:2: UnresolvedAmpPath: AmpPath &missing could not be resolved to a Definition"
        );
    }

    fn sample_forest() -> Forest {
        Forest {
            files: vec![SourceFile {
                id: FileId(0),
                grove: 0,
                root: PathBuf::from("/tmp/grove"),
                path: PathBuf::from("/tmp/grove/notes.md"),
                bytes: Vec::new(),
                text: String::new(),
            }],
            definitions: vec![Definition {
                id: DefinitionId(0),
                path: "chimp:parser".to_string(),
                is_phony: false,
                file: Some(FileId(0)),
                line: Some(1),
                date: None,
                order: Some(12),
                assignee: Some("geert".to_string()),
                wbs: vec!["project".to_string()],
            }],
            chores: vec![Chore {
                file: FileId(0),
                line: 2,
                column: 1,
                text: "- [ ] Build parser".to_string(),
                status: Some(Status::Todo),
                date: None,
                order: None,
                assignee: None,
                wbs: Vec::new(),
                definitions: vec![DefinitionId(0)],
            }],
            issues: Vec::new(),
        }
    }
}
