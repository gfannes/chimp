use std::fs;
use std::path::{Path, PathBuf};

use chimp::{
    CheckIssue, Chore, ComputedOrder, Config, ExportOptions, Forest, GroveConfig, OrderMetadata,
    Status, build_forest, computed_chore_order, export_forest,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
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
            let mut details = false;
            let mut limit = None;

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
                    "-d" | "--details" => details = true,
                    "-n" | "--limit" => {
                        let value = args.next().ok_or("-n requires a count")?;
                        limit = Some(parse_count(&value)?);
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

            let cli_config = default_cli_config_with_extra(extra_roots)?;
            let forest = build_forest(&Config::from_groves(cli_config.groves.clone()))?;
            print_chore_search_results(
                &forest,
                status_filter,
                &assignee_filters,
                &query_terms,
                color,
                details,
                limit,
                cli_config.default_assignee.as_deref(),
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
            print_wbs_results(&forest, color);
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
        "debug" => {
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
            print_debug(&forest, color);
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
                    chimp::naft::decode_to_base(&nodes, base)?;
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

fn default_groves_with_extra(extra_roots: Vec<PathBuf>) -> Result<Vec<GroveConfig>> {
    Ok(default_cli_config_with_extra(extra_roots)?.groves)
}

fn read_config_groves_from_default_locations() -> Result<Option<Vec<GroveConfig>>> {
    Ok(read_cli_config_from_default_locations()?.map(|config| config.groves))
}

#[derive(Debug, Clone, Default)]
struct CliConfig {
    groves: Vec<GroveConfig>,
    default_assignee: Option<String>,
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
    if config.groves.is_empty() && config.default_assignee.is_none() {
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
    }
}

fn read_cli_config(path: &Path) -> Result<CliConfig> {
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let text = std::fs::read_to_string(path)?;
    Ok(parse_config(base, &text))
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

fn parse_config(base: &Path, text: &str) -> CliConfig {
    let mut config = CliConfig::default();
    let mut current = None::<GroveDraft>;

    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line == "[[grove]]" || line == "[[groves]]" {
            if let Some(draft) = current.take().and_then(|draft| draft.finish(base)) {
                config.groves.push(draft);
            }
            current = Some(GroveDraft::default());
            continue;
        }

        if let Some(draft) = current.as_mut() {
            apply_grove_config_line(draft, line);
        } else {
            if let Some(value) = config_value(line, "default_assignee") {
                config.default_assignee = quoted_value(value).map(str::to_string);
            }
            config.groves.extend(
                parse_root_config_line(line)
                    .into_iter()
                    .map(|root| GroveConfig::from_root(config_root_path(base, root))),
            );
        }
    }

    if let Some(draft) = current.and_then(|draft| draft.finish(base)) {
        config.groves.push(draft);
    }

    config
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
    Ok(value.parse::<u8>()?)
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

fn print_debug(forest: &Forest, color: bool) {
    println!(
        "files={} definitions={} chores={} issues={}",
        forest.files.len(),
        forest.definitions.len(),
        forest.chores.len(),
        forest.issues.len()
    );
    println!("files");
    for file in &forest.files {
        println!(
            "  {}: grove={} path={} bytes={}",
            file.id.0,
            file.grove,
            file.path.display(),
            file.bytes.len()
        );
    }
    println!("definitions");
    for definition in &forest.definitions {
        println!(
            "  {}: path={} phony={} location={} order={} assignee={} wbs={}",
            definition.id.0,
            definition.path,
            definition.is_phony,
            definition_location(forest, definition.file, definition.line),
            metadata_order_label(definition.order),
            option_str(definition.assignee.as_deref()),
            list_or_dash(&definition.wbs)
        );
    }
    println!("chores");
    for chore in &forest.chores {
        let defs = chore
            .definitions
            .iter()
            .map(|id| forest.definitions[id.0].path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let order = chore_order(forest, chore);
        println!(
            "  {}",
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
    println!("issues");
    for issue in &forest.issues {
        println!("  {}", format_check_issue(forest, issue));
    }
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
    if status_filter.is_some_and(|status| chore.status != Some(status)) {
        return false;
    }
    if !assignee_filters.is_empty()
        && !assignee_filters.iter().any(|assignee| {
            effective_chore_assignees(forest, chore, default_assignee)
                .iter()
                .any(|candidate| *candidate == assignee.as_str())
        })
    {
        return false;
    }
    query_terms
        .iter()
        .all(|term| searchable_text_contains(forest, chore, term))
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
    if let Some(assignee) = chore.assignee.as_deref() {
        assignees.push(assignee);
    }
    for id in &chore.definitions {
        if let Some(assignee) = forest.definitions[id.0].assignee.as_deref() {
            if !assignees.contains(&assignee) {
                assignees.push(assignee);
            }
        }
    }
    if assignees.is_empty() {
        if let Some(assignee) = default_assignee {
            assignees.push(assignee);
        }
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
    !chore.wbs.is_empty()
        || chore
            .definitions
            .iter()
            .any(|id| !forest.definitions[id.0].wbs.is_empty())
}

fn print_wbs_results(forest: &Forest, color: bool) {
    print_chore_results(forest, color, false, None, None, |forest, chore| {
        chore_has_wbs(forest, chore)
    });
}

fn print_chore_search_results(
    forest: &Forest,
    status_filter: Option<Status>,
    assignee_filters: &[String],
    query_terms: &[String],
    color: bool,
    details: bool,
    limit: Option<usize>,
    default_assignee: Option<&str>,
) {
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
    );
}

fn print_chore_results(
    forest: &Forest,
    color: bool,
    details: bool,
    limit: Option<usize>,
    default_assignee: Option<&str>,
    matches: impl Fn(&Forest, &Chore) -> bool,
) {
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

    let mut printed = 0usize;
    let mut current_file = None;
    let mut current_order = None;
    for chore in chores {
        if limit.is_some_and(|limit| printed >= limit) {
            break;
        }
        let order = chore_order(forest, chore);
        if current_file != Some(chore.file) {
            current_file = Some(chore.file);
            current_order = None;
            println!("{}", forest.files[chore.file.0].path.display());
        }
        if details && current_order != Some(order) {
            current_order = Some(order);
            println!("  {}", colored_order(order, color));
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
        println!("    {}", colorize(&line, order, color));
        if details {
            println!(
                "      details: computed_{} status={} assignee={} date={} wbs={}",
                order_label(order),
                status_label(chore.status),
                assignee_label(forest, chore, default_assignee),
                option_str(chore.date.as_deref()),
                list_or_dash(&chore.wbs)
            );
            println!(
                "      definitions: {}",
                if defs.is_empty() { "-" } else { &defs }
            );
        }
        printed += 1;
    }
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
        "chimp\n\nUSAGE:\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] scan [PATH...]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] groves [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] check [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] debug [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] wbs [-C PATH]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] chores [-d|--details] [-n COUNT] [-C PATH] [--status STATUS] [--assignee NAME|@NAME] [QUERY...]\n  chimp [--nocolor] [-V LEVEL|--verbose LEVEL] export DEST [--include-amp|--strip-amp] [--status STATUS] [--amp TAG] [--ext EXT] [PATH...]\n  chimp [-V LEVEL|--verbose LEVEL] naft encode OUT.naft [-u] [-U] FOLDER...\n  chimp [-V LEVEL|--verbose LEVEL] naft decode IN.naft BASE_FOLDER\n"
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
        let config = parse_config(
            Path::new("/tmp/config"),
            r#"
default_assignee = "fallback"

[[grove]]
path = "docs"
extensions = ["md", ".txt"]
max_filesize = 1024

[[grove]]
root = "/abs/src"
extensions = ["rs"]
"#,
        );

        assert_eq!(config.default_assignee.as_deref(), Some("fallback"));
        assert_eq!(config.groves.len(), 2);
        assert_eq!(config.groves[0].root, PathBuf::from("/tmp/config/docs"));
        assert_eq!(config.groves[0].extensions, vec!["md", "txt"]);
        assert_eq!(config.groves[0].max_filesize, Some(1024));
        assert_eq!(config.groves[1].root, PathBuf::from("/abs/src"));
        assert_eq!(config.groves[1].extensions, vec!["rs"]);
        assert_eq!(config.groves[1].max_filesize, None);
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
                order: Some(OrderMetadata {
                    value: 12,
                    exclusive: false,
                }),
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
