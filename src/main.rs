use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chimp::{
    CheckIssue, Chore, ComputedOrder, Config, ExportOptions, Forest, GroveConfig, OrderMetadata,
    Status, amp_path_depth, build_forest_with_reporter_without_occurrences, computed_chore_order,
    export_forest,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

static VERBOSITY: AtomicU8 = AtomicU8::new(1);

fn main() {
    if let Err(error) = run() {
        if VERBOSITY.load(Ordering::Relaxed) >= 1 {
            eprintln!("Error: {error}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut color = true;
    let mut verbose = 1u8;
    let mut command = None;
    while let Some(arg) = args.next() {
        if arg == "--nocolor" {
            color = false;
        } else if arg == "-V" || arg == "--verbose" {
            let value = args.next().ok_or("-V requires a level")?;
            verbose = parse_verbose_level(&value)?;
            VERBOSITY.store(verbose, Ordering::Relaxed);
        } else {
            command = Some(arg);
            break;
        }
    }
    let Some(command) = command else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "scan" => {
            let config = Config::from_groves(parse_path_groves(args)?);
            let forest = build_cli_forest(&config, verbose, false)?;
            println!(
                "Files: {}; Definitions: {}; Chores: {}",
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
            let mut details = false;
            let mut limit = None;
            let mut edit = false;

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-h" | "--help" => {
                        print_chores_help();
                        return Ok(());
                    }
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-s" | "--status" => {
                        let value = args.next().ok_or("--status requires a value")?;
                        status_filter = Some(parse_status(&value)?);
                    }
                    "-a" | "--assignee" => {
                        assignee_filters.push(
                            args.next()
                                .ok_or("--assignee requires a value")?
                                .to_lowercase(),
                        );
                    }
                    "-d" | "--details" => details = true,
                    "-e" | "--edit" => edit = true,
                    "-n" | "--limit" => {
                        let value = args.next().ok_or("-n requires a count")?;
                        limit = Some(parse_count(&value)?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ if arg.starts_with('@') && arg.len() > 1 => {
                        assignee_filters.push(arg.trim_start_matches('@').to_lowercase());
                    }
                    _ => query_terms.push(arg),
                }
            }

            let cli_config = default_cli_config_with_extra(extra_roots)?;
            let forest = build_cli_forest(
                &Config::from_groves(cli_config.groves.clone()),
                verbose,
                false,
            )?;
            let locations = print_chore_search_results(
                &forest,
                status_filter,
                &assignee_filters,
                &query_terms,
                color,
                details,
                limit,
                cli_config.default_assignee.as_deref(),
            );
            if edit {
                open_editor_locations(cli_config.editor.as_deref(), &locations, limit)?;
            }
        }
        "wbs" => {
            let mut extra_roots = Vec::new();
            let mut edit = false;
            let mut limit = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-e" | "--edit" => edit = true,
                    "-n" | "--limit" => {
                        limit = Some(parse_count(&args.next().ok_or("-n requires a count")?)?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            let cli_config = default_cli_config_with_extra(extra_roots)?;
            let forest = build_cli_forest(
                &Config::from_groves(cli_config.groves.clone()),
                verbose,
                false,
            )?;
            let locations = print_wbs_results(&forest, color);
            if edit {
                open_editor_locations(cli_config.editor.as_deref(), &locations, limit)?;
            }
        }
        "config" => {
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
            print_config(&default_cli_config_with_extra(extra_roots)?);
        }
        "check" => {
            let mut extra_roots = Vec::new();
            let mut edit = false;
            let mut limit = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-e" | "--edit" => edit = true,
                    "-n" | "--limit" => {
                        limit = Some(parse_count(&args.next().ok_or("-n requires a count")?)?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            let cli_config = default_cli_config_with_extra(extra_roots)?;
            let forest = build_cli_forest(
                &Config::from_groves(cli_config.groves.clone()),
                verbose,
                true,
            )?;
            let locations = print_check_issues(&forest);
            if edit {
                open_editor_locations(cli_config.editor.as_deref(), &locations, limit)?;
            }
        }
        "debug" => {
            let mut extra_roots = Vec::new();
            let mut edit = false;
            let mut limit = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-e" | "--edit" => edit = true,
                    "-n" | "--limit" => {
                        limit = Some(parse_count(&args.next().ok_or("-n requires a count")?)?);
                    }
                    _ if arg.starts_with('-') => {
                        return Err(format!("unexpected argument: {arg}").into());
                    }
                    _ => extra_roots.push(PathBuf::from(arg)),
                }
            }
            let cli_config = default_cli_config_with_extra(extra_roots)?;
            let forest = build_cli_forest(
                &Config::from_groves(cli_config.groves.clone()),
                verbose,
                false,
            )?;
            print_debug(&forest, color);
            if edit {
                let locations = debug_locations(&forest);
                open_editor_locations(cli_config.editor.as_deref(), &locations, limit)?;
            }
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
            let forest =
                build_cli_forest(&Config::from_groves(default_groves(roots)?), verbose, false)?;
            let summary = export_forest(&forest, destination, &options)?;
            println!("Files written: {}", summary.files_written);
        }
        "lsp" => {
            let mut extra_roots = Vec::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-C" | "--grove" => {
                        extra_roots.push(PathBuf::from(args.next().ok_or("-C requires a path")?));
                    }
                    "-h" | "--help" => {
                        println!("chimp lsp\n\nUSAGE:\n  chimp lsp [-C PATH|--grove PATH]\n");
                        return Ok(());
                    }
                    _ => return Err(format!("unexpected argument: {arg}").into()),
                }
            }
            let cli_config = default_cli_config_with_extra(extra_roots)?;
            chimp::lsp::run(
                Config::from_groves(cli_config.groves),
                chimp::lsp::Options {
                    max_array_size: cli_config.lsp_max_array_size.unwrap_or(100),
                    verbose,
                },
            )?;
        }
        "naft" => {
            let Some(subcommand) = args.next() else {
                return Err("naft requires encode or decode".into());
            };
            match subcommand.as_str() {
                "encode" => {
                    let out = PathBuf::from(args.next().ok_or("naft encode requires OUT.naft")?);
                    let mut folders = Vec::new();
                    let mut include_hidden = false;
                    let mut include_ignored = false;
                    for arg in args {
                        match arg.as_str() {
                            "-u" => include_hidden = true,
                            "-U" => include_ignored = true,
                            _ if arg.starts_with('-') => {
                                return Err(format!("unexpected argument: {arg}").into());
                            }
                            _ => folders.push(PathBuf::from(arg)),
                        }
                    }
                    if folders.is_empty() {
                        return Err("naft encode requires at least one folder".into());
                    }
                    let nodes = chimp::naft::encode_folders_with_options(
                        &folders,
                        &chimp::naft::EncodeOptions {
                            verbose,
                            include_hidden,
                            include_ignored,
                        },
                        |path| eprintln!("processing {}", path.display()),
                    )?;
                    fs::write(out, chimp::naft::serialize_document(&nodes))?;
                }
                "decode" => {
                    let input = args.next().ok_or("naft decode requires IN.naft")?;
                    let base = args.next().ok_or("naft decode requires BASE_FOLDER")?;
                    if args.next().is_some() {
                        return Err("unexpected argument after naft decode BASE_FOLDER".into());
                    }
                    let text = fs::read_to_string(input)?;
                    let nodes = chimp::naft::parse_document(&text)?;
                    chimp::naft::decode_to_base_with_reporter(&nodes, base, verbose, |path| {
                        eprintln!("processing {}", path.display())
                    })?;
                }
                _ => return Err(format!("unknown naft command: {subcommand}").into()),
            }
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

fn read_config_groves_from_default_locations() -> Result<Option<Vec<GroveConfig>>> {
    Ok(read_cli_config_from_default_locations()?.map(|config| config.groves))
}

#[derive(Debug, Clone, Default)]
struct CliConfig {
    groves: Vec<GroveConfig>,
    default_assignee: Option<String>,
    editor: Option<String>,
    lsp_max_array_size: Option<usize>,
}

fn default_cli_config_with_extra(extra_roots: Vec<PathBuf>) -> Result<CliConfig> {
    let mut config = read_cli_config_from_default_locations()?.unwrap_or_default();
    config
        .groves
        .extend(extra_roots.into_iter().map(GroveConfig::from_root));
    if config.groves.is_empty() {
        config
            .groves
            .push(GroveConfig::from_root(std::env::current_dir()?));
    }
    Ok(config)
}

fn read_cli_config_from_default_locations() -> Result<Option<CliConfig>> {
    let mut config = CliConfig::default();
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join(".config/chimp/config.toml");
        config.merge(read_cli_config(&path)?);
    }
    config.merge(read_cli_config(Path::new("chimp.toml"))?);
    if config.groves.is_empty()
        && config.default_assignee.is_none()
        && config.editor.is_none()
        && config.lsp_max_array_size.is_none()
    {
        Ok(None)
    } else {
        Ok(Some(config))
    }
}

impl CliConfig {
    fn merge(&mut self, other: CliConfig) {
        self.groves.extend(other.groves);
        if other.default_assignee.is_some() {
            self.default_assignee = other.default_assignee;
        }
        if other.editor.is_some() {
            self.editor = other.editor;
        }
        if other.lsp_max_array_size.is_some() {
            self.lsp_max_array_size = other.lsp_max_array_size;
        }
    }
}

fn read_cli_config(path: &Path) -> Result<CliConfig> {
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
    parse_config(base, &text)
        .map_err(|error| format!("failed to parse config {}: {error}", path.display()).into())
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
    line: usize,
}

impl GroveDraft {
    fn finish(self, base: &Path) -> Result<GroveConfig> {
        let root = self
            .root
            .ok_or_else(|| format!("line {}: Grove is missing `path`", self.line))?;
        Ok(GroveConfig {
            root: config_root_path(base, root),
            extensions: self.extensions,
            max_filesize: self.max_filesize,
        })
    }
}

fn parse_config(base: &Path, text: &str) -> Result<CliConfig> {
    let mut config = CliConfig::default();
    let mut current = None::<GroveDraft>;
    let mut in_lsp = false;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line == "[[grove]]" || line == "[[groves]]" {
            if let Some(draft) = current.take() {
                config.groves.push(draft.finish(base)?);
            }
            current = Some(GroveDraft {
                line: line_number,
                ..GroveDraft::default()
            });
            in_lsp = false;
            continue;
        }
        if line == "[lsp]" {
            if let Some(draft) = current.take() {
                config.groves.push(draft.finish(base)?);
            }
            in_lsp = true;
            continue;
        }
        if line.starts_with('[') {
            return Err(format!("line {line_number}: unsupported table `{line}`").into());
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {line_number}: expected `key = value`"))?;
        let key = key.trim();
        let value = value.trim();

        if in_lsp {
            match key {
                "max_array_size" => {
                    let size = value.parse::<usize>().map_err(|_| {
                        format!("line {line_number}: `max_array_size` must be a positive integer")
                    })?;
                    if size == 0 {
                        return Err(format!(
                            "line {line_number}: `max_array_size` must be greater than zero"
                        )
                        .into());
                    }
                    config.lsp_max_array_size = Some(size);
                }
                _ => {
                    return Err(format!("line {line_number}: unknown LSP key `{key}`").into());
                }
            }
        } else if let Some(draft) = current.as_mut() {
            apply_grove_config_value(draft, key, value, line_number)?;
        } else {
            match key {
                "default_assignee" => {
                    config.default_assignee = Some(parse_quoted(value, line_number)?.to_string());
                }
                "editor" => config.editor = Some(parse_quoted(value, line_number)?.to_string()),
                "root" => config.groves.push(GroveConfig::from_root(config_root_path(
                    base,
                    PathBuf::from(parse_quoted(value, line_number)?),
                ))),
                "roots" => {
                    config.groves.extend(
                        parse_string_array(value, line_number)?
                            .into_iter()
                            .map(PathBuf::from)
                            .map(|root| GroveConfig::from_root(config_root_path(base, root))),
                    );
                }
                _ => {
                    return Err(format!("line {line_number}: unknown top-level key `{key}`").into());
                }
            }
        }
    }

    if let Some(draft) = current {
        config.groves.push(draft.finish(base)?);
    }

    Ok(config)
}

fn apply_grove_config_value(
    draft: &mut GroveDraft,
    key: &str,
    value: &str,
    line: usize,
) -> Result<()> {
    match key {
        "path" | "root" => draft.root = Some(PathBuf::from(parse_quoted(value, line)?)),
        "extensions" | "includes" => {
            draft.extensions = parse_string_array(value, line)?
                .into_iter()
                .map(|value| value.trim_start_matches('.').to_string())
                .collect();
        }
        "max_filesize" | "max_file_size" => {
            draft.max_filesize = Some(
                value
                    .parse::<u64>()
                    .map_err(|_| format!("line {line}: `{key}` must be a non-negative integer"))?,
            );
        }
        _ => return Err(format!("line {line}: unknown Grove key `{key}`").into()),
    }
    Ok(())
}

fn parse_string_array(value: &str, line: usize) -> Result<Vec<String>> {
    let value = value.trim();
    let Some(value) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(format!("line {line}: expected an array of quoted strings").into());
    };
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|value| parse_quoted(value, line).map(str::to_string))
        .collect()
}

fn parse_quoted(value: &str, line: usize) -> Result<&str> {
    value
        .trim()
        .trim_matches(',')
        .trim()
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("line {line}: expected a quoted string").into())
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}

fn parse_status(value: &str) -> Result<Status> {
    match value.to_ascii_uppercase().as_str() {
        "TODO" => Ok(Status::Todo),
        "GO" => Ok(Status::Go),
        "WIP" => Ok(Status::Wip),
        "DONE" => Ok(Status::Done),
        "QUESTION" => Ok(Status::Question),
        "INFO" => Ok(Status::Info),
        "BLOCKED" => Ok(Status::Blocked),
        "FORWARD" => Ok(Status::Forward),
        "PLANNED" => Ok(Status::Planned),
        "CANCELED" | "CANCELLED" => Ok(Status::Canceled),
        "ASSIGNED" => Ok(Status::Assigned),
        _ => Err("status must be TODO, GO, WIP, DONE, QUESTION, INFO, BLOCKED, FORWARD, PLANNED, CANCELED, or ASSIGNED".into()),
    }
}

fn parse_count(value: &str) -> Result<usize> {
    let count = value.parse::<usize>()?;
    if count == 0 {
        return Err("count must be greater than zero".into());
    }
    Ok(count)
}

fn parse_verbose_level(value: &str) -> Result<u8> {
    let level = value.parse::<u8>()?;
    if level > 4 {
        return Err("verbose level must be between 0 and 4".into());
    }
    Ok(level)
}

fn build_cli_forest(config: &Config, verbose: u8, checking: bool) -> Result<Forest> {
    let forest = build_forest_with_reporter_without_occurrences(config, verbose, |path| {
        eprintln!("processing {}", path.display());
    })?;
    if verbose >= 2 && !checking && !forest.issues.is_empty() {
        eprintln!(
            "warning: scan found {} potentially suspicious issue(s); run `chimp check` for details",
            forest.issues.len()
        );
    }
    if verbose >= 4 {
        for definition in &forest.definitions {
            eprintln!("definition {}", definition.path);
        }
        for chore in &forest.chores {
            eprintln!(
                "chore {}:{}",
                forest.files[chore.file.0].path.display(),
                chore.line
            );
        }
    }
    Ok(forest)
}

fn print_config(config: &CliConfig) {
    println!("# Chimp configuration");
    println!(
        "Default assignee: {}",
        config
            .default_assignee
            .as_deref()
            .map(|assignee| format!("`{assignee}`"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Editor: {}",
        config
            .editor
            .as_deref()
            .map(|editor| format!("`{editor}`"))
            .unwrap_or_else(|| "`$EDITOR` or `nvim`".to_string())
    );
    println!(
        "LSP maximum array size: {}",
        config.lsp_max_array_size.unwrap_or(100)
    );
    println!("## Groves");
    for (index, grove) in config.groves.iter().enumerate() {
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
            "{}. `{}` — extensions: `{}`, max filesize: `{}`",
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

#[derive(Debug, Clone)]
struct FileLocation {
    file: chimp::FileId,
    path: PathBuf,
    line: usize,
    column: usize,
}

fn print_check_issues(forest: &Forest) -> Vec<FileLocation> {
    let mut locations = Vec::new();
    for issue in &forest.issues {
        println!("- {}", format_check_issue(forest, issue));
        if let Some(file) = issue.file
            && !locations
                .iter()
                .any(|location: &FileLocation| location.file == file)
        {
            locations.push(FileLocation {
                file,
                path: forest.files[file.0].path.clone(),
                line: issue.line.unwrap_or(1),
                column: 1,
            });
        }
    }
    println!("Issues: {}", forest.issues.len());
    locations
}

fn format_check_issue(forest: &Forest, issue: &CheckIssue) -> String {
    let location = match (issue.file, issue.line) {
        (Some(file), Some(line)) => format!("{}:{line}", forest.files[file.0].path.display()),
        (Some(file), None) => forest.files[file.0].path.display().to_string(),
        (None, Some(line)) => format!("<unknown>:{line}"),
        (None, None) => "<unknown>".to_string(),
    };
    format!("`{location}` — **{:?}**: {}", issue.kind, issue.message)
}

fn print_debug(forest: &Forest, color: bool) {
    println!(
        "Files: {}; Definitions: {}; Chores: {}; Issues: {}",
        forest.files.len(),
        forest.definitions.len(),
        forest.chores.len(),
        forest.issues.len()
    );
    println!("## Files");
    for file in &forest.files {
        println!(
            "- {}: grove={}, path=`{}`, bytes={}",
            file.id.0,
            file.grove,
            file.path.display(),
            file.bytes.len()
        );
    }
    println!("## Definitions");
    for definition in &forest.definitions {
        let injected = definition
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>();
        println!(
            "- {}: path=`{}`; phony={}; exclusive={}; assignee_definition={}; location=`{}`; order={}; assignee={}{}; wbs={}; injected=[{}]",
            definition.id.0,
            definition.path,
            definition.is_phony,
            definition.exclusive,
            definition.is_assignee,
            definition_location(forest, definition.file, definition.line),
            metadata_order_label(definition.order),
            option_str(definition.assignee.as_deref()),
            if definition.assignee_exclusive {
                " exclusive"
            } else {
                ""
            },
            list_or_dash(&definition.wbs),
            injected.join(", ")
        );
    }
    println!("## Chores");
    for chore in &forest.chores {
        let defs = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let order = chore_order(forest, chore);
        println!(
            "- {}",
            colorize(
                &format!(
                    "{}:{}:{} order={} status={} assignee={} date={} wbs={} defs=[{}] text={}",
                    forest.files[chore.file.0].path.display(),
                    chore.line,
                    chore.column,
                    order_label(order),
                    status_label(chore.status),
                    option_str(chore.assignee.as_deref()),
                    option_str(chore.date.as_deref()),
                    list_or_dash(&chore.wbs),
                    defs,
                    chore.text.trim()
                ),
                order,
                color
            )
        );
    }
    println!("## Issues");
    for issue in &forest.issues {
        println!("- {}", format_check_issue(forest, issue));
    }
}

fn debug_locations(forest: &Forest) -> Vec<FileLocation> {
    forest
        .files
        .iter()
        .map(|file| {
            let chore_location = forest
                .chores
                .iter()
                .filter(|chore| chore.file == file.id)
                .map(|chore| (chore.line, chore.column))
                .min();
            let definition_location = forest
                .definitions
                .iter()
                .filter(|definition| definition.file == Some(file.id))
                .filter_map(|definition| definition.line.map(|line| (line, 1)))
                .min();
            let (line, column) = chore_location.or(definition_location).unwrap_or((1, 1));
            FileLocation {
                file: file.id,
                path: file.path.clone(),
                line,
                column,
            }
        })
        .collect()
}

fn open_editor_locations(
    configured_editor: Option<&str>,
    locations: &[FileLocation],
    limit: Option<usize>,
) -> Result<()> {
    let editor = configured_editor
        .filter(|editor| !editor.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|editor| !editor.trim().is_empty())
        })
        .unwrap_or_else(|| "nvim".to_string());
    let editor_parts = editor.split_whitespace().collect::<Vec<_>>();
    let Some(program) = editor_parts.first() else {
        return Err("editor command is empty".into());
    };

    for location in locations.iter().take(limit.unwrap_or(usize::MAX)) {
        let path = location.path.to_string_lossy();
        let line = location.line.to_string();
        let column = location.column.to_string();
        let has_placeholders = editor_parts.iter().any(|part| {
            part.contains("{file}") || part.contains("{line}") || part.contains("{column}")
        });
        let mut command = Command::new(program);
        if has_placeholders {
            command.args(editor_parts.iter().skip(1).map(|part| {
                part.replace("{file}", &path)
                    .replace("{line}", &line)
                    .replace("{column}", &column)
            }));
        } else {
            command.args(editor_parts.iter().skip(1));
            let editor_name = Path::new(program)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(program);
            match editor_name {
                "nvim" | "vim" | "vi" => {
                    command.arg(format!("+call cursor({line},{column})"));
                    command.arg(path.as_ref());
                }
                "code" | "codium" => {
                    command.arg("--goto");
                    command.arg(format!("{path}:{line}:{column}"));
                }
                "emacs" | "emacsclient" => {
                    command.arg(format!("+{line}:{column}"));
                    command.arg(path.as_ref());
                }
                _ => {
                    command.arg(format!("{path}:{line}:{column}"));
                }
            }
        }
        let status = command
            .status()
            .map_err(|error| format!("failed to start editor `{editor}`: {error}"))?;
        if !status.success() {
            return Err(format!("editor `{editor}` exited with status {status}").into());
        }
    }
    Ok(())
}

fn definition_location(
    forest: &Forest,
    file: Option<chimp::FileId>,
    line: Option<usize>,
) -> String {
    match (file, line) {
        (Some(file), Some(line)) => format!("{}:{line}", forest.files[file.0].path.display()),
        (Some(file), None) => forest.files[file.0].path.display().to_string(),
        _ => "-".to_string(),
    }
}

fn chore_matches(
    forest: &Forest,
    chore: &Chore,
    status_filter: Option<Status>,
    assignee_filters: &[String],
    query_terms: &[String],
    default_assignee: Option<&str>,
) -> bool {
    if !chore.status.is_some_and(chore_status_is_reported) {
        return false;
    }
    if !chore_is_visible(forest, chore) {
        return false;
    }
    if status_filter.is_some_and(|status| chore.status != Some(status)) {
        return false;
    }
    if !assignee_filters.is_empty()
        && !assignee_filters.iter().any(|assignee| {
            effective_chore_assignees(forest, chore, default_assignee)
                .iter()
                .any(|candidate| candidate.to_lowercase() == assignee.to_lowercase())
        })
    {
        return false;
    }
    query_terms.iter().all(|term| {
        if let Some(text) = term.strip_prefix("text:") {
            !text.is_empty()
                && chore
                    .text
                    .to_ascii_lowercase()
                    .contains(&text.to_ascii_lowercase())
        } else {
            searchable_text_contains(forest, chore, term)
        }
    })
}

fn chore_status_is_reported(status: Status) -> bool {
    matches!(
        status,
        Status::Todo | Status::Go | Status::Wip | Status::Question | Status::Blocked
    )
}

fn effective_chore_assignees<'a>(
    forest: &'a Forest,
    chore: &'a Chore,
    default_assignee: Option<&'a str>,
) -> Vec<&'a str> {
    let mut assignees = Vec::new();
    let mut definitions = chore.definitions.to_vec();
    definitions.sort_by_key(|id| amp_path_depth(&forest.definitions[id.0].path));
    for id in definitions {
        let definition = &forest.definitions[id.0];
        if let Some(assignee) = definition.assignee.as_deref() {
            if definition.assignee_exclusive {
                assignees.clear();
            }
            if !assignees.contains(&assignee) {
                assignees.push(assignee);
            }
        }
    }
    if let Some(assignee) = chore.assignee.as_deref() {
        if chore.assignee_exclusive {
            assignees.clear();
        }
        if !assignees.contains(&assignee) {
            assignees.push(assignee);
        }
    }
    if assignees.is_empty()
        && let Some(assignee) = default_assignee
    {
        assignees.push(assignee);
    }
    assignees
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

fn chore_order(forest: &Forest, chore: &Chore) -> Option<ComputedOrder> {
    computed_chore_order(forest, chore)
}

fn chore_has_wbs(forest: &Forest, chore: &Chore) -> bool {
    chore_is_visible(forest, chore)
        && (!chore.wbs.is_empty()
            || chore
                .definitions
                .iter()
                .any(|id| !forest.definitions[id.0].wbs.is_empty()))
}

fn chore_is_visible(forest: &Forest, chore: &Chore) -> bool {
    static TODAY: OnceLock<String> = OnceLock::new();
    chore_is_visible_on(forest, chore, TODAY.get_or_init(current_date))
}

fn chore_is_visible_on(forest: &Forest, chore: &Chore, today: &str) -> bool {
    let earliest = chore
        .date
        .as_deref()
        .into_iter()
        .chain(
            chore
                .definitions
                .iter()
                .filter_map(|id| forest.definitions[id.0].date.as_deref()),
        )
        .min();
    earliest.is_none_or(|date| date <= today)
}

fn current_date() -> String {
    if let Ok(output) = Command::new("date").arg("+%Y%m%d").output()
        && output.status.success()
    {
        let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if date.len() == 8 && date.chars().all(|ch| ch.is_ascii_digit()) {
            return date;
        }
    }
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}{month:02}{day:02}")
}

fn civil_date_from_unix_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u32, day as u32)
}

fn print_wbs_results(forest: &Forest, color: bool) -> Vec<FileLocation> {
    print_chore_results(forest, color, false, None, None, |forest, chore| {
        chore_has_wbs(forest, chore)
    })
}

#[allow(clippy::too_many_arguments)]
fn print_chore_search_results(
    forest: &Forest,
    status_filter: Option<Status>,
    assignee_filters: &[String],
    query_terms: &[String],
    color: bool,
    details: bool,
    limit: Option<usize>,
    default_assignee: Option<&str>,
) -> Vec<FileLocation> {
    print_chore_results(
        forest,
        color,
        details,
        limit,
        default_assignee,
        |forest, chore| {
            chore_matches(
                forest,
                chore,
                status_filter,
                assignee_filters,
                query_terms,
                default_assignee,
            )
        },
    )
}

fn print_chore_results(
    forest: &Forest,
    color: bool,
    details: bool,
    limit: Option<usize>,
    default_assignee: Option<&str>,
    matches: impl Fn(&Forest, &Chore) -> bool,
) -> Vec<FileLocation> {
    let mut chores = forest
        .chores
        .iter()
        .filter(|chore| matches(forest, chore))
        .collect::<Vec<_>>();
    chores.sort_by(|left, right| {
        chore_sort_key(forest, left)
            .cmp(&chore_sort_key(forest, right))
            .then_with(|| {
                forest.files[left.file.0]
                    .path
                    .cmp(&forest.files[right.file.0].path)
            })
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.column.cmp(&right.column))
    });

    let mut current_file = None;
    let mut current_order = None;
    let mut locations = Vec::new();
    for (printed, chore) in chores.into_iter().enumerate() {
        if limit.is_some_and(|limit| printed >= limit) {
            break;
        }
        let order = chore_order(forest, chore);
        if current_file != Some(chore.file) {
            current_file = Some(chore.file);
            current_order = None;
            println!("## `{}`", forest.files[chore.file.0].path.display());
            locations.push(FileLocation {
                file: chore.file,
                path: forest.files[chore.file.0].path.clone(),
                line: chore.line,
                column: chore.column,
            });
        }
        if details && current_order != Some(order) {
            current_order = Some(order);
            println!("### Order {}", colored_order(order, color));
        }
        let defs = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let line = if details {
            format!(
                "{}  [{}]",
                chore.text.trim(),
                chore_tag_info(forest, chore, order, &defs, default_assignee)
            )
        } else {
            chore.text.trim().to_string()
        };
        let line = colorize(&line, order, color);
        if is_markdown_enumeration(&line) {
            println!("{line}");
        } else {
            println!("- {line}");
        }
        if details {
            println!(
                "  - details: computed_{} status={} assignee={} date={} wbs={}",
                order_label(order),
                status_label(chore.status),
                assignee_label(forest, chore, default_assignee),
                option_str(chore.date.as_deref()),
                list_or_dash(&chore.wbs)
            );
            println!(
                "  - definitions: {}",
                if defs.is_empty() { "-" } else { &defs }
            );
        }
    }
    locations
}

fn is_markdown_enumeration(line: &str) -> bool {
    let plain = line.trim_start_matches(|ch: char| {
        ch == '\x1b' || ch == '[' || ch.is_ascii_digit() || ch == ';' || ch == 'm'
    });
    let trimmed = plain.trim_start();
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.split_once(". ").is_some_and(|(number, _)| {
            !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn chore_tag_info(
    forest: &Forest,
    chore: &Chore,
    order: Option<ComputedOrder>,
    defs: &str,
    default_assignee: Option<&str>,
) -> String {
    format!(
        "{}:{} {} status={} assignee={} date={} wbs={} defs={}",
        chore.line,
        chore.column,
        order_label(order),
        status_label(chore.status),
        assignee_label(forest, chore, default_assignee),
        option_str(chore.date.as_deref()),
        list_or_dash(&chore.wbs),
        if defs.is_empty() { "-" } else { defs }
    )
}

fn assignee_label(forest: &Forest, chore: &Chore, default_assignee: Option<&str>) -> String {
    let assignees = effective_chore_assignees(forest, chore, default_assignee);
    if assignees.is_empty() {
        "-".to_string()
    } else {
        assignees.join(",")
    }
}

fn chore_sort_key(forest: &Forest, chore: &Chore) -> (u8, std::cmp::Reverse<u32>) {
    chore_order(forest, chore)
        .map(|order| (1, std::cmp::Reverse(order.value)))
        .unwrap_or((0, std::cmp::Reverse(0)))
}

fn colored_order(order: Option<ComputedOrder>, color: bool) -> String {
    let label = order_label(order);
    colorize(&label, order, color)
}

fn colorize(text: &str, order: Option<ComputedOrder>, color: bool) -> String {
    if !color {
        return text.to_string();
    }
    match order {
        Some(order) => format!("\x1b[{}m{text}\x1b[0m", order_color(order.value)),
        None => format!("\x1b[90m{text}\x1b[0m"),
    }
}

fn order_label(order: Option<ComputedOrder>) -> String {
    match order {
        Some(ComputedOrder {
            value,
            exclusive: true,
            conflict: true,
        }) => format!("order={value} exclusive conflict"),
        Some(ComputedOrder {
            value,
            exclusive: true,
            conflict: false,
        }) => format!("order={value} exclusive"),
        Some(order) => format!("order={}", order.value),
        None => "order=-".to_string(),
    }
}

fn metadata_order_label(order: Option<OrderMetadata>) -> String {
    match order {
        Some(OrderMetadata {
            value,
            exclusive: true,
        }) => format!("{value} exclusive"),
        Some(order) => order.value.to_string(),
        None => "-".to_string(),
    }
}

fn status_label(status: Option<Status>) -> &'static str {
    match status {
        Some(Status::Todo) => "TODO",
        Some(Status::Go) => "GO",
        Some(Status::Done) => "DONE",
        Some(Status::Question) => "QUESTION",
        Some(Status::Info) => "INFO",
        Some(Status::Wip) => "WIP",
        Some(Status::Blocked) => "BLOCKED",
        Some(Status::Forward) => "FORWARD",
        Some(Status::Planned) => "PLANNED",
        Some(Status::Canceled) => "CANCELED",
        Some(Status::Assigned) => "ASSIGNED",
        None => "-",
    }
}

fn option_str(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn list_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(",")
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
        "chimp\n\nUSAGE:\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] scan [PATH...]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] config [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] check [-e] [-n COUNT] [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] debug [-e] [-n COUNT] [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] wbs [-e] [-n COUNT] [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] chores [-e] [-d|--details] [-n COUNT] [-C PATH] [--status STATUS] [--assignee NAME|@NAME] [QUERY...]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] export DEST [--include-amp|--strip-amp] [--status STATUS] [--amp TAG] [--ext EXT] [PATH...]\n  chimp [-V LEVEL|--verbose LEVEL] lsp [-C PATH]\n  chimp [-V LEVEL|--verbose LEVEL] naft encode OUT.naft [-u] [-U] FOLDER...\n  chimp [-V LEVEL|--verbose LEVEL] naft decode IN.naft BASE_FOLDER\n"
    );
}

fn print_chores_help() {
    println!(
        "chimp chores\n\nUSAGE:\n  chimp chores [OPTIONS] [QUERY...]\n\nQUERY:\n  TERM                 Match TERM case-insensitively in Chore text, file paths,\n                       related Definition paths, or Definition assignees.\n  text:TERM            Match only the raw Chore-line text. Quote phrases, for\n                       example 'text:release blocker'.\n  @NAME                Match Chores assigned to NAME. Multiple assignees use OR.\n\nAll ordinary QUERY terms use AND. Assignee terms use OR with each other and AND\nwith ordinary terms. Definition paths use their normalized colon-separated form.\n\nOPTIONS:\n  -C, --grove PATH      Add a Grove root (repeatable)\n  -s, --status STATUS  Filter by status\n  -a, --assignee NAME  Filter by assignee (repeatable)\n  -d, --details        Show order and related metadata\n  -e, --edit           Open matching locations in the configured editor\n  -n, --limit COUNT    Limit reported Chores (and edited files)\n  -h, --help           Print this help\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chimp::{Chore, Definition, DefinitionId, FileId, SourceFile};

    #[test]
    fn parses_config_roots() {
        let config = parse_config(
            Path::new("/tmp/config"),
            "root = \"projects/one\"\nroots = [\"a\", \"b\"]\n",
        )
        .unwrap();
        assert_eq!(
            config
                .groves
                .iter()
                .map(|grove| grove.root.clone())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("/tmp/config/projects/one"),
                PathBuf::from("/tmp/config/a"),
                PathBuf::from("/tmp/config/b")
            ]
        );
    }

    #[test]
    fn parses_lsp_config() {
        let config = parse_config(
            Path::new("/tmp/config"),
            "root = \"notes\"\n[lsp]\nmax_array_size = 42\n",
        )
        .unwrap();
        assert_eq!(config.lsp_max_array_size, Some(42));
        assert!(parse_config(Path::new("/tmp/config"), "[lsp]\nmax_array_size = 0\n").is_err());
    }

    #[test]
    fn parses_per_grove_config() {
        let config = parse_config(
            Path::new("/tmp/config"),
            r#"
default_assignee = "fallback"
editor = "nvim --clean"

[[grove]]
path = "docs"
extensions = ["md", ".txt"]
max_filesize = 1024

[[grove]]
root = "/abs/src"
extensions = ["rs"]
"#,
        )
        .unwrap();

        assert_eq!(config.default_assignee.as_deref(), Some("fallback"));
        assert_eq!(config.editor.as_deref(), Some("nvim --clean"));
        assert_eq!(config.groves.len(), 2);
        assert_eq!(config.groves[0].root, PathBuf::from("/tmp/config/docs"));
        assert_eq!(config.groves[0].extensions, vec!["md", "txt"]);
        assert_eq!(config.groves[0].max_filesize, Some(1024));
        assert_eq!(config.groves[1].root, PathBuf::from("/abs/src"));
        assert_eq!(config.groves[1].extensions, vec!["rs"]);
        assert_eq!(config.groves[1].max_filesize, None);
    }

    #[test]
    fn config_parser_reports_line_for_invalid_values() {
        let error = parse_config(
            Path::new("/tmp/config"),
            "[[grove]]\npath = \"docs\"\nmax_filesize = many\n",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("line 3"));
        assert!(error.contains("max_filesize"));
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
            &["parser".to_string()],
            None
        ));
        assert!(!chore_matches(
            &forest,
            chore,
            Some(Status::Todo),
            &["someone".to_string()],
            &["parser".to_string()],
            None
        ));
        assert!(!chore_matches(
            &forest,
            chore,
            Some(Status::Done),
            &[],
            &["parser".to_string()],
            None
        ));
    }

    #[test]
    fn default_assignee_matches_unassigned_chores() {
        let mut forest = sample_forest();
        forest.definitions[0].assignee = None;
        let chore = &forest.chores[0];

        assert!(chore_matches(
            &forest,
            chore,
            Some(Status::Todo),
            &["fallback".to_string()],
            &[],
            Some("fallback")
        ));
        assert!(!chore_matches(
            &forest,
            chore,
            Some(Status::Todo),
            &["geert".to_string()],
            &[],
            Some("fallback")
        ));
    }

    #[test]
    fn chore_visibility_uses_earliest_direct_or_related_date() {
        let mut forest = sample_forest();
        forest.chores[0].date = Some("20990101".to_string());
        forest.definitions[0].date = Some("20260101".to_string());
        assert!(chore_is_visible_on(&forest, &forest.chores[0], "20260806"));

        forest.definitions[0].date = Some("20980101".to_string());
        assert!(!chore_is_visible_on(&forest, &forest.chores[0], "20260806"));

        forest.chores[0].date = None;
        forest.definitions[0].date = None;
        assert!(chore_is_visible_on(&forest, &forest.chores[0], "20260806"));
    }

    #[test]
    fn converts_unix_days_to_calendar_date() {
        assert_eq!(civil_date_from_unix_days(0), (1970, 1, 1));
        assert_eq!(civil_date_from_unix_days(20_671), (2026, 8, 6));
    }

    #[test]
    fn max_order_comes_from_resolved_definitions() {
        let forest = sample_forest();
        assert_eq!(chore_order(&forest, &forest.chores[0]).unwrap().value, 12);
    }

    #[test]
    fn wbs_match_uses_chore_or_definition_metadata() {
        let forest = sample_forest();
        assert!(chore_has_wbs(&forest, &forest.chores[0]));
    }

    #[test]
    fn order_labels_are_color_coded() {
        let order = Some(ComputedOrder {
            value: 1,
            exclusive: false,
            conflict: false,
        });
        assert_eq!(colored_order(order, false), "order=1");
        assert_eq!(colored_order(None, false), "order=-");
        assert_eq!(colored_order(order, true), "\x1b[32morder=1\x1b[0m");
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
            "`/tmp/grove/notes.md:2` — **UnresolvedAmpPath**: AmpPath &missing could not be resolved to a Definition"
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
                text: std::sync::Arc::new(String::new()),
            }],
            definitions: vec![Definition {
                id: DefinitionId(0),
                path: "chimp:parser".to_string(),
                is_phony: false,
                exclusive: false,
                is_assignee: false,
                file: Some(FileId(0)),
                line: Some(1),
                date: None,
                order: Some(OrderMetadata {
                    value: 12,
                    exclusive: false,
                }),
                assignee: Some("geert".to_string()),
                assignee_exclusive: false,
                wbs: vec!["project".to_string()],
                definitions: Vec::new(),
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
                assignee_exclusive: false,
                wbs: Vec::new(),
                definitions: vec![DefinitionId(0)],
            }],
            amp_occurrences: Vec::new(),
            issues: Vec::new(),
        }
    }
}
