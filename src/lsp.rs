use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::{AmpOccurrence, Chore, Config, FileId, Forest, Status};

pub struct Options {
    pub max_array_size: usize,
    pub verbose: u8,
}

struct Workspace {
    config: Config,
    overlays: HashMap<PathBuf, String>,
    versions: HashMap<PathBuf, i64>,
    forest: Forest,
    dirty: bool,
    refreshed_at: Instant,
}

impl Workspace {
    fn new(config: Config) -> crate::Result<Self> {
        let forest = crate::build_forest(&config)?;
        Ok(Self {
            config,
            overlays: HashMap::new(),
            versions: HashMap::new(),
            forest,
            dirty: false,
            refreshed_at: Instant::now(),
        })
    }

    fn refresh(&mut self) -> crate::Result<()> {
        if self.dirty || self.refreshed_at.elapsed() >= Duration::from_secs(300) {
            self.forest = crate::build_forest_with_overlays(&self.config, &self.overlays)?;
            self.dirty = false;
            self.refreshed_at = Instant::now();
        }
        Ok(())
    }

    fn path_for_uri(&self, uri: &str) -> Result<PathBuf, String> {
        uri_to_path(uri)
    }
}

pub fn run(config: Config, options: Options) -> crate::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run_with_io(stdin.lock(), stdout.lock(), config, options)
}

pub fn run_with_io(
    input: impl Read,
    mut output: impl Write,
    config: Config,
    options: Options,
) -> crate::Result<()> {
    let mut input = std::io::BufReader::new(input);
    let mut workspace = Workspace::new(config)?;
    let mut shutdown = false;
    while let Some(message) = read_message(&mut input)? {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        if id.is_none() {
            match method {
                "initialized" => {}
                "exit" => break,
                "textDocument/didOpen" => did_open(&mut workspace, &params),
                "textDocument/didChange" => did_change(&mut workspace, &params),
                "textDocument/didClose" => did_close(&mut workspace, &params),
                _ if options.verbose >= 2 => {
                    eprintln!("warning: unhandled LSP notification `{method}`")
                }
                _ => {}
            }
            continue;
        }
        let id = id.unwrap();
        let result = match method {
            "initialize" => Ok(initialize_result()),
            "shutdown" => {
                shutdown = true;
                Ok(Value::Null)
            }
            _ if shutdown => Err((-32600, "server has been shut down".to_string())),
            "textDocument/completion" => semantic(&mut workspace, completion),
            "textDocument/definition" => {
                semantic(&mut workspace, |forest| definition(forest, &params))
            }
            "textDocument/references" => {
                semantic(&mut workspace, |forest| references(forest, &params))
            }
            "textDocument/documentSymbol" => {
                semantic(&mut workspace, |forest| document_symbols(forest, &params))
            }
            "workspace/symbol" => semantic(&mut workspace, |forest| {
                workspace_symbols(forest, &params, options.max_array_size)
            }),
            "textDocument/declaration" => semantic(&mut workspace, |forest| {
                task_locations(forest, &params, TaskMode::Local)
            }),
            "textDocument/implementation" => semantic(&mut workspace, |forest| {
                task_locations(forest, &params, TaskMode::All)
            }),
            "textDocument/typeDefinition" => semantic(&mut workspace, |forest| {
                task_locations(forest, &params, TaskMode::FirstPerSegment)
            }),
            "textDocument/codeAction" => {
                Ok(json!([{ "title": "Reload Chimp workspace", "command": "reload" }]))
            }
            "workspace/executeCommand" => {
                if params.get("command").and_then(Value::as_str) == Some("reload") {
                    workspace.dirty = true;
                    workspace
                        .refresh()
                        .map(|()| Value::Null)
                        .map_err(internal_error)
                } else {
                    Err((-32602, "unsupported command".to_string()))
                }
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        let response = match result {
            Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
            }
        };
        write_message(&mut output, &response)?;
    }
    Ok(())
}

fn semantic(
    workspace: &mut Workspace,
    f: impl FnOnce(&Forest) -> Result<Value, (i32, String)>,
) -> Result<Value, (i32, String)> {
    workspace.refresh().map_err(internal_error)?;
    f(&workspace.forest)
}

fn internal_error(error: impl std::fmt::Display) -> (i32, String) {
    (-32603, error.to_string())
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "positionEncoding": "utf-16",
            "textDocumentSync": {"openClose":true,"change":2},
            "completionProvider": {"triggerCharacters":["&"]},
            "documentSymbolProvider": true,
            "workspaceSymbolProvider": true,
            "definitionProvider": true,
            "declarationProvider": true,
            "implementationProvider": true,
            "typeDefinitionProvider": true,
            "referencesProvider": true,
            "codeActionProvider": true,
            "executeCommandProvider": {"commands":["reload"]}
        },
        "serverInfo": {"name":"chimp", "version":env!("CARGO_PKG_VERSION")}
    })
}

fn did_open(workspace: &mut Workspace, params: &Value) {
    let Some(document) = params.get("textDocument") else {
        return;
    };
    let (Some(uri), Some(text)) = (
        document.get("uri").and_then(Value::as_str),
        document.get("text").and_then(Value::as_str),
    ) else {
        return;
    };
    if let Ok(path) = workspace.path_for_uri(uri) {
        workspace.overlays.insert(path, text.to_string());
        if let Some(version) = document.get("version").and_then(Value::as_i64) {
            workspace
                .versions
                .insert(uri_to_path(uri).unwrap(), version);
        }
        workspace.dirty = true;
    }
}

fn did_close(workspace: &mut Workspace, params: &Value) {
    let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
        return;
    };
    if let Ok(path) = workspace.path_for_uri(uri) {
        workspace.overlays.remove(&path);
        workspace.versions.remove(&path);
        workspace.dirty = true;
    }
}

fn did_change(workspace: &mut Workspace, params: &Value) {
    let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
        return;
    };
    let Ok(path) = workspace.path_for_uri(uri) else {
        return;
    };
    if let Some(version) = params
        .pointer("/textDocument/version")
        .and_then(Value::as_i64)
    {
        if workspace
            .versions
            .get(&path)
            .is_some_and(|current| version <= *current)
        {
            return;
        }
        workspace.versions.insert(path.clone(), version);
    }
    let Some(mut text) = workspace
        .overlays
        .get(&path)
        .cloned()
        .or_else(|| std::fs::read_to_string(&path).ok())
    else {
        return;
    };
    let Some(changes) = params.get("contentChanges").and_then(Value::as_array) else {
        return;
    };
    for change in changes {
        let Some(replacement) = change.get("text").and_then(Value::as_str) else {
            continue;
        };
        if let Some(range) = change.get("range") {
            if let (Some(start), Some(end)) = (
                position_offset(&text, &range["start"]),
                position_offset(&text, &range["end"]),
            ) {
                text.replace_range(start..end, replacement);
            }
        } else {
            text = replacement.to_string();
        }
    }
    workspace.overlays.insert(path, text);
    workspace.dirty = true;
}

fn completion(forest: &Forest) -> Result<Value, (i32, String)> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for definition in &forest.definitions {
        let label = amp_tail(&definition.path).trim_matches('`');
        if seen.insert(label.to_string()) {
            items.push(json!({"label":label,"kind":14}));
        }
    }
    Ok(Value::Array(items))
}

fn requested_occurrence<'a>(forest: &'a Forest, params: &Value) -> Option<&'a AmpOccurrence> {
    let path = uri_to_path(params.pointer("/textDocument/uri")?.as_str()?).ok()?;
    let file = forest
        .files
        .iter()
        .find(|file| same_path(&file.path, &path))?;
    let position = params.get("position")?;
    let line = position.get("line")?.as_u64()? as usize + 1;
    let character = position.get("character")?.as_u64()? as usize;
    forest.amp_occurrences.iter().find(|occurrence| {
        occurrence.file == file.id && occurrence.line == line && {
            let start = utf16_column(forest, occurrence.file, line, occurrence.start_column);
            let end = utf16_column(forest, occurrence.file, line, occurrence.end_column);
            start <= character && character < end
        }
    })
}

fn definition(forest: &Forest, params: &Value) -> Result<Value, (i32, String)> {
    let Some(selected) = requested_occurrence(forest, params) else {
        return Ok(Value::Null);
    };
    let target = &forest.definitions[selected.definition.0];
    let occurrence = forest.amp_occurrences.iter().find(|item| {
        item.is_declaration
            && item.definition == target.id
            && item.file == target.file.unwrap_or(item.file)
            && item.line == target.line.unwrap_or(item.line)
    });
    Ok(occurrence
        .map(|item| occurrence_location(forest, item))
        .unwrap_or(Value::Null))
}

fn references(forest: &Forest, params: &Value) -> Result<Value, (i32, String)> {
    let Some(selected) = requested_occurrence(forest, params) else {
        return Ok(Value::Null);
    };
    let include = params
        .pointer("/context/includeDeclaration")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(Value::Array(
        forest
            .amp_occurrences
            .iter()
            .filter(|item| {
                item.definition == selected.definition && (include || !item.is_declaration)
            })
            .map(|item| occurrence_location(forest, item))
            .collect(),
    ))
}

fn document_symbols(forest: &Forest, params: &Value) -> Result<Value, (i32, String)> {
    let path = param_path(params)?;
    let Some(file) = forest
        .files
        .iter()
        .find(|file| same_path(&file.path, &path))
    else {
        return Ok(json!([]));
    };
    let mut result = Vec::new();
    for chore in forest
        .chores
        .iter()
        .filter(|chore| chore.file == file.id && chore.status != Some(Status::Done))
    {
        let direct = forest
            .amp_occurrences
            .iter()
            .filter(|item| item.file == file.id && item.line == chore.line)
            .collect::<Vec<_>>();
        if let (Some(first), Some(last)) = (direct.first(), direct.last()) {
            result.push(json!({"name":chore.text.trim(),"kind":7,"range":range_for_occurrences(forest, first, last),"selectionRange":range_for_occurrences(forest, first, last)}));
        }
    }
    Ok(Value::Array(result))
}

fn workspace_symbols(
    forest: &Forest,
    params: &Value,
    limit: usize,
) -> Result<Value, (i32, String)> {
    let query = params.get("query").and_then(Value::as_str).unwrap_or("");
    let mut matches = forest
        .chores
        .iter()
        .filter_map(|chore| {
            let haystack = format!(
                "{} {}",
                chore.text,
                chore
                    .definitions
                    .iter()
                    .map(|id| forest.definitions[id.0].path.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            fuzzy_score(query, &haystack).map(|score| (score, chore))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(score, _)| *score);
    Ok(Value::Array(matches.into_iter().take(limit).map(|(_, chore)| json!({"name":chore.text.trim(),"kind":7,"location":chore_location(forest, chore)})).collect()))
}

enum TaskMode {
    Local,
    All,
    FirstPerSegment,
}

fn task_locations(forest: &Forest, params: &Value, mode: TaskMode) -> Result<Value, (i32, String)> {
    let local = param_path(params).ok();
    let today = current_date();
    let mut chores = forest
        .chores
        .iter()
        .filter(|chore| active_chore(forest, chore, &today))
        .collect::<Vec<_>>();
    chores.sort_by_key(|chore| {
        (
            crate::computed_chore_order(forest, chore).map(|order| std::cmp::Reverse(order.value)),
            forest.files[chore.file.0].path.clone(),
            chore.line,
        )
    });
    let mut previous = None;
    let values = chores
        .into_iter()
        .filter(|chore| match mode {
            TaskMode::Local => local
                .as_ref()
                .is_some_and(|path| same_path(&forest.files[chore.file.0].path, path)),
            TaskMode::All => true,
            TaskMode::FirstPerSegment => {
                let include = previous != Some(chore.file);
                previous = Some(chore.file);
                include
            }
        })
        .map(|chore| chore_location(forest, chore))
        .collect();
    Ok(Value::Array(values))
}

fn active_chore(forest: &Forest, chore: &Chore, today: &str) -> bool {
    matches!(
        chore.status,
        Some(Status::Todo | Status::Go | Status::Wip | Status::Question | Status::Blocked)
    ) && chore
        .date
        .as_deref()
        .into_iter()
        .chain(
            chore
                .definitions
                .iter()
                .filter_map(|id| forest.definitions[id.0].date.as_deref()),
        )
        .min()
        .is_none_or(|date| date <= today)
}

fn current_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or(0)
        + 719_468;
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
    format!("{year:04}{month:02}{day:02}")
}

fn occurrence_location(forest: &Forest, occurrence: &AmpOccurrence) -> Value {
    json!({"uri":path_to_uri(&forest.files[occurrence.file.0].path),"range":range_for_occurrences(forest, occurrence, occurrence)})
}

fn range_for_occurrences(forest: &Forest, first: &AmpOccurrence, last: &AmpOccurrence) -> Value {
    json!({"start":{"line":first.line-1,"character":utf16_column(forest,first.file,first.line,first.start_column)},"end":{"line":last.line-1,"character":utf16_column(forest,last.file,last.line,last.end_column)}})
}

fn chore_location(forest: &Forest, chore: &Chore) -> Value {
    let start = utf16_column(forest, chore.file, chore.line, chore.column);
    json!({"uri":path_to_uri(&forest.files[chore.file.0].path),"range":{"start":{"line":chore.line-1,"character":start},"end":{"line":chore.line-1,"character":start+chore.text.encode_utf16().count()}}})
}

fn utf16_column(forest: &Forest, file: FileId, line: usize, byte_column: usize) -> usize {
    forest.files[file.0]
        .text
        .lines()
        .nth(line.saturating_sub(1))
        .map(|text| {
            text.get(..byte_column.saturating_sub(1))
                .unwrap_or(text)
                .encode_utf16()
                .count()
        })
        .unwrap_or(0)
}

fn param_path(params: &Value) -> Result<PathBuf, (i32, String)> {
    params
        .pointer("/textDocument/uri")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "expected textDocument.uri".to_string()))
        .and_then(|uri| uri_to_path(uri).map_err(|message| (-32602, message)))
}

fn fuzzy_score(needle: &str, haystack: &str) -> Option<usize> {
    let mut offset = 0;
    let haystack = haystack.to_ascii_lowercase();
    for ch in needle
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
    {
        let found = haystack[offset..].find(ch)?;
        offset += found + ch.len_utf8();
    }
    Some(offset)
}

fn amp_tail(path: &str) -> &str {
    path.rsplit(':').next().unwrap_or(path)
}

fn position_offset(text: &str, position: &Value) -> Option<usize> {
    let line = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    let line_start = text
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>();
    let content = text.get(line_start..)?.split('\n').next().unwrap_or("");
    let mut units = 0;
    for (index, ch) in content.char_indices() {
        if units >= character {
            return Some(line_start + index);
        }
        units += ch.len_utf16();
    }
    (units == character).then_some(line_start + content.len())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}

fn uri_to_path(uri: &str) -> Result<PathBuf, String> {
    let raw = uri
        .strip_prefix("file://")
        .ok_or_else(|| "only file:// URIs are supported".to_string())?;
    let mut bytes = Vec::new();
    let raw = raw.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[index + 1..index + 3])
                .map_err(|_| "invalid URI escape")?;
            bytes.push(u8::from_str_radix(hex, 16).map_err(|_| "invalid URI escape")?);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    let path = PathBuf::from(String::from_utf8(bytes).map_err(|_| "file URI is not UTF-8")?);
    Ok(path.canonicalize().unwrap_or(path))
}

fn path_to_uri(path: &Path) -> String {
    let mut result = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn read_message(input: &mut impl BufRead) -> crate::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.trim().strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>()?);
        }
    }
    let mut body = vec![0; content_length.ok_or("missing Content-Length header")?];
    input.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

fn write_message(output: &mut impl Write, value: &Value) -> crate::Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("chimp-{name}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn frame(value: Value) -> Vec<u8> {
        let body = serde_json::to_vec(&value).unwrap();
        format!("Content-Length: {}\r\n\r\n", body.len())
            .bytes()
            .chain(body)
            .collect()
    }

    fn responses(mut bytes: &[u8]) -> Vec<Value> {
        let mut result = Vec::new();
        while !bytes.is_empty() {
            let marker = bytes
                .windows(4)
                .position(|item| item == b"\r\n\r\n")
                .unwrap();
            let header = std::str::from_utf8(&bytes[..marker]).unwrap();
            let len = header
                .strip_prefix("Content-Length: ")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let start = marker + 4;
            result.push(serde_json::from_slice(&bytes[start..start + len]).unwrap());
            bytes = &bytes[start + len..];
        }
        result
    }

    #[test]
    fn protocol_lifecycle_and_completion_are_framed() {
        let dir = test_dir("lsp-lifecycle");
        fs::write(dir.join("notes.md"), "# Alpha &&:alpha\n").unwrap();
        let mut input = Vec::new();
        input.extend(frame(
            json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{}}),
        ));
        input.extend(frame(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"context":{"triggerCharacter":"&"}}})));
        input.extend(frame(json!({"jsonrpc":"2.0","id":3,"method":"shutdown"})));
        input.extend(frame(json!({"jsonrpc":"2.0","method":"exit"})));
        let mut output = Vec::new();
        run_with_io(
            input.as_slice(),
            &mut output,
            Config::from_roots(vec![dir.clone()]),
            Options {
                max_array_size: 100,
                verbose: 0,
            },
        )
        .unwrap();
        let output = responses(&output);
        assert_eq!(output[0]["id"], "init");
        assert_eq!(
            output[0]["result"]["capabilities"]["positionEncoding"],
            "utf-16"
        );
        assert!(
            output[1]["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == "alpha")
        );
        assert!(output[2]["result"].is_null());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unsaved_incremental_change_updates_definition_navigation() {
        let dir = test_dir("lsp-overlay");
        let path = dir.join("notes.md");
        fs::write(
            &path,
            "# Alpha &&:alpha\n# Beta &&:beta\n- [ ] &alpha task\n",
        )
        .unwrap();
        let uri = path_to_uri(&path);
        let mut input = Vec::new();
        input.extend(frame(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        ));
        input.extend(frame(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":"# Alpha &&:alpha\n# Beta &&:beta\n- [ ] &alpha task\n"}}})));
        input.extend(frame(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"range":{"start":{"line":2,"character":7},"end":{"line":2,"character":12}},"text":"beta"}]}})));
        input.extend(frame(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":8}}})));
        input.extend(frame(json!({"jsonrpc":"2.0","id":3,"method":"shutdown"})));
        input.extend(frame(json!({"jsonrpc":"2.0","method":"exit"})));
        let mut output = Vec::new();
        run_with_io(
            input.as_slice(),
            &mut output,
            Config::from_roots(vec![dir.clone()]),
            Options {
                max_array_size: 100,
                verbose: 0,
            },
        )
        .unwrap();
        let output = responses(&output);
        assert_eq!(output[1]["result"]["range"]["start"]["line"], 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
