use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;

#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeOptions {
    pub verbose: u8,
    pub include_hidden: bool,
    pub include_ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub tag: String,
    pub options: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn folder(name: impl Into<String>, children: Vec<Node>) -> Self {
        Self {
            tag: "Folder".to_string(),
            options: vec![("name".to_string(), name.into())],
            children,
        }
    }

    pub fn file(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tag: "File".to_string(),
            options: vec![
                ("name".to_string(), name.into()),
                ("content".to_string(), content.into()),
            ],
            children: Vec::new(),
        }
    }

    pub fn binary_file(name: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            tag: "File".to_string(),
            options: vec![
                ("name".to_string(), name.into()),
                ("content_base64".to_string(), encode_base64(bytes)),
            ],
            children: Vec::new(),
        }
    }

    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }
}

pub fn parse_document(text: &str) -> Result<Vec<Node>> {
    let mut parser = Parser { text, pos: 0 };
    let nodes = parser.parse_nodes(None)?;
    parser.skip_ws();
    if parser.pos != parser.text.len() {
        return Err(format!("unexpected input at byte {}", parser.pos).into());
    }
    Ok(nodes)
}

pub fn serialize_document(nodes: &[Node]) -> String {
    let mut output = String::new();
    for node in nodes {
        serialize_node(node, 0, &mut output);
    }
    output
}

pub fn encode_folders(paths: &[PathBuf]) -> Result<Vec<Node>> {
    encode_folders_with_options(paths, &EncodeOptions::default(), |_| {})
}

pub fn encode_folders_with_options(
    paths: &[PathBuf],
    options: &EncodeOptions,
    mut log: impl FnMut(&Path),
) -> Result<Vec<Node>> {
    paths
        .iter()
        .map(|path| encode_folder(path, path, options, Vec::new(), &mut log))
        .collect()
}

pub fn decode_to_base(nodes: &[Node], base: impl AsRef<Path>) -> Result<()> {
    decode_to_base_with_reporter(nodes, base, 0, |_| {})
}

pub fn decode_to_base_with_reporter(
    nodes: &[Node],
    base: impl AsRef<Path>,
    verbose: u8,
    mut log: impl FnMut(&Path),
) -> Result<()> {
    let base = base.as_ref();
    let mut targets = Vec::new();
    for node in nodes {
        collect_targets(node, base, &mut targets)?;
    }
    if let Some(existing) = targets.iter().find(|target| target.exists()) {
        return Err(format!("decode target already exists: {}", existing.display()).into());
    }
    for node in nodes {
        write_node(node, base, verbose, &mut log)?;
    }
    Ok(())
}

fn encode_folder(
    root: &Path,
    path: &Path,
    options: &EncodeOptions,
    inherited_ignore: Vec<IgnorePattern>,
    log: &mut impl FnMut(&Path),
) -> Result<Node> {
    if options.verbose >= 3 {
        log(path);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("folder has no valid name: {}", path.display()))?;
    let mut ignore = inherited_ignore;
    if !options.include_ignored {
        ignore.extend(read_gitignore(root, path)?);
    }
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());
    let mut children = Vec::new();
    for entry in entries {
        let file_type = entry.file_type()?;
        let path = entry.path();
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| format!("path has no valid UTF-8 name: {}", path.display()))?
            .to_string();
        if !options.include_hidden && name.starts_with('.') {
            continue;
        }
        if !options.include_ignored && ignored(root, &path, &name, &ignore) {
            continue;
        }
        if file_type.is_dir() {
            children.push(encode_folder(root, &path, options, ignore.clone(), log)?);
        } else if file_type.is_file() {
            if options.verbose >= 3 {
                log(&path);
            }
            let bytes = fs::read(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            match String::from_utf8(bytes) {
                Ok(content) => children.push(Node::file(name, content)),
                Err(err) => children.push(Node::binary_file(name, err.as_bytes())),
            }
        }
    }
    Ok(Node::folder(name, children))
}

fn collect_targets(node: &Node, base: &Path, targets: &mut Vec<PathBuf>) -> Result<()> {
    let path = child_path(base, node)?;
    targets.push(path.clone());
    for child in &node.children {
        collect_targets(child, &path, targets)?;
    }
    Ok(())
}

fn write_node(node: &Node, base: &Path, verbose: u8, log: &mut impl FnMut(&Path)) -> Result<()> {
    let path = child_path(base, node)?;
    if verbose >= 3 {
        log(&path);
    }
    match node.tag.as_str() {
        "Folder" => {
            fs::create_dir(&path)?;
            for child in &node.children {
                write_node(child, &path, verbose, log)?;
            }
        }
        "File" => {
            if let Some(content) = node.option("content") {
                fs::write(&path, content)?;
            } else if let Some(content) = node.option("content_base64") {
                fs::write(&path, decode_base64(content)?)?;
            } else {
                fs::write(&path, "")?;
            }
        }
        _ => return Err(format!("unsupported naft filesystem tag: {}", node.tag).into()),
    }
    Ok(())
}

fn child_path(base: &Path, node: &Node) -> Result<PathBuf> {
    let name = node
        .option("name")
        .ok_or_else(|| format!("{} node is missing name option", node.tag))?;
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(format!("invalid naft node name: {name}").into());
    }
    Ok(base.join(name))
}

#[derive(Debug, Clone)]
struct IgnorePattern {
    raw: String,
    base: PathBuf,
    anchored: bool,
}

fn read_gitignore(root: &Path, dir: &Path) -> Result<Vec<IgnorePattern>> {
    let path = dir.join(".gitignore");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| IgnorePattern {
            raw: line.trim_matches('/').to_string(),
            base: dir.strip_prefix(root).unwrap_or(dir).to_path_buf(),
            anchored: line.starts_with('/') || line.trim_matches('/').contains('/'),
        })
        .collect())
}

fn ignored(root: &Path, path: &Path, name: &str, patterns: &[IgnorePattern]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    patterns.iter().any(|pattern| {
        let from_base = path
            .strip_prefix(root.join(&pattern.base))
            .unwrap_or(path)
            .to_string_lossy();
        (!pattern.anchored && pattern.raw == name)
            || pattern.raw == from_base
            || pattern.raw == relative
            || (pattern.raw.starts_with("*.") && name.ends_with(&pattern.raw[1..]))
    })
}

fn serialize_node(node: &Node, indent: usize, output: &mut String) {
    output.push_str(&"  ".repeat(indent));
    output.push('[');
    output.push_str(&escape_balanced(&node.tag, '[', ']'));
    output.push(']');
    for (key, value) in &node.options {
        output.push('(');
        output.push_str(&escape_key(key));
        output.push(':');
        output.push_str(&escape_balanced(value, '(', ')'));
        output.push(')');
    }
    if node.children.is_empty() {
        output.push('\n');
    } else {
        output.push_str("{\n");
        for child in &node.children {
            serialize_node(child, indent + 1, output);
        }
        output.push_str(&"  ".repeat(indent));
        output.push_str("}\n");
    }
}

fn escape_key(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | '(' | ')' | ':') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn escape_balanced(text: &str, open: char, close: char) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut escape = vec![false; chars.len()];
    let mut stack = Vec::new();

    for (index, ch) in chars.iter().copied().enumerate() {
        if ch == open {
            stack.push(index);
        } else if ch == close && stack.pop().is_none() {
            escape[index] = true;
        }
    }
    for index in stack {
        escape[index] = true;
    }

    let mut escaped = String::new();
    for (index, ch) in chars.into_iter().enumerate() {
        if ch == '\\' || escape[index] {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        encoded.push(BASE64[(b0 >> 2) as usize] as char);
        encoded.push(BASE64[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(BASE64[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(BASE64[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

fn decode_base64(text: &str) -> Result<Vec<u8>> {
    let cleaned = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<Vec<_>>();
    if cleaned.len() % 4 != 0 {
        return Err("base64 content length must be a multiple of 4".into());
    }
    let mut decoded = Vec::new();
    for chunk in cleaned.chunks(4) {
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == '=' {
            None
        } else {
            Some(base64_value(chunk[2])?)
        };
        let d = if chunk[3] == '=' {
            None
        } else {
            Some(base64_value(chunk[3])?)
        };
        decoded.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            decoded.push(((b & 0b0000_1111) << 4) | (c >> 2));
            if let Some(d) = d {
                decoded.push(((c & 0b0000_0011) << 6) | d);
            }
        } else if d.is_some() {
            return Err("invalid base64 padding".into());
        }
    }
    Ok(decoded)
}

fn base64_value(ch: char) -> Result<u8> {
    match ch {
        'A'..='Z' => Ok(ch as u8 - b'A'),
        'a'..='z' => Ok(ch as u8 - b'a' + 26),
        '0'..='9' => Ok(ch as u8 - b'0' + 52),
        '+' => Ok(62),
        '/' => Ok(63),
        _ => Err(format!("invalid base64 character: {ch}").into()),
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn parse_nodes(&mut self, terminator: Option<char>) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            if terminator.is_some_and(|ch| self.peek() == Some(ch)) {
                self.pos += 1;
                return Ok(nodes);
            }
            if self.peek().is_none() {
                if terminator.is_some() {
                    return Err("unclosed naft node body".into());
                }
                return Ok(nodes);
            }
            nodes.push(self.parse_node()?);
        }
    }

    fn parse_node(&mut self) -> Result<Node> {
        self.expect('[')?;
        let tag = self.read_balanced_until('[', ']')?;
        self.expect(']')?;
        let mut options = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some('(') {
                break;
            }
            self.pos += 1;
            let key = self.read_escaped_until(':')?;
            self.expect(':')?;
            let value = self.read_balanced_until('(', ')')?;
            self.expect(')')?;
            options.push((key, value));
        }
        self.skip_ws();
        let children = if self.peek() == Some('{') {
            self.pos += 1;
            self.parse_nodes(Some('}'))?
        } else {
            Vec::new()
        };
        Ok(Node {
            tag,
            options,
            children,
        })
    }

    fn read_escaped_until(&mut self, end: char) -> Result<String> {
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == end {
                return Ok(value);
            }
            self.pos += ch.len_utf8();
            if ch == '\\' {
                let Some(escaped) = self.peek() else {
                    return Err("dangling naft escape".into());
                };
                if !matches!(escaped, '\\' | '(' | ')' | '{' | '}' | '[' | ']' | ':') {
                    return Err(format!("unknown naft escape: \\{escaped}").into());
                }
                self.pos += escaped.len_utf8();
                value.push(escaped);
            } else {
                value.push(ch);
            }
        }
        Err(format!("missing naft terminator: {end}").into())
    }

    fn read_balanced_until(&mut self, open: char, close: char) -> Result<String> {
        let mut value = String::new();
        let mut depth = 0usize;
        while let Some(ch) = self.peek() {
            if ch == close && depth == 0 {
                return Ok(value);
            }
            self.pos += ch.len_utf8();
            if ch == '\\' {
                let Some(escaped) = self.peek() else {
                    return Err("dangling naft escape".into());
                };
                if !matches!(escaped, '\\' | '(' | ')' | '{' | '}' | '[' | ']' | ':') {
                    return Err(format!("unknown naft escape: \\{escaped}").into());
                }
                self.pos += escaped.len_utf8();
                value.push(escaped);
            } else if ch == open {
                depth += 1;
                value.push(ch);
            } else if ch == close {
                depth -= 1;
                value.push(ch);
            } else {
                value.push(ch);
            }
        }
        Err(format!("missing naft terminator: {close}").into())
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += self.peek().unwrap().len_utf8();
        }
    }

    fn expect(&mut self, expected: char) -> Result<()> {
        if self.peek() != Some(expected) {
            return Err(format!("expected naft character: {expected}").into());
        }
        self.pos += expected.len_utf8();
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_and_serializes_nested_nodes_with_escapes() {
        let text = "[Folder](name:root){[File](name:a\\)b.md)(content:x\\: \\[y\\] \\\\ z)}";
        let nodes = parse_document(text).unwrap();
        assert_eq!(nodes[0].option("name"), Some("root"));
        assert_eq!(nodes[0].children[0].option("name"), Some("a)b.md"));
        assert_eq!(nodes[0].children[0].option("content"), Some("x: [y] \\ z"));
        let serialized = serialize_document(&nodes);
        assert!(serialized.contains("[Folder](name:root){"));
        assert!(serialized.contains("a\\)b.md"));
    }

    #[test]
    fn values_keep_non_structural_punctuation_readable() {
        let node = Node::file(
            "code.rs",
            "use std::path::{Path, PathBuf};\n- [ ] TODO &task\nfn main() { call(a, b) }\n",
        );

        let serialized = serialize_document(&[node]);

        assert!(serialized.contains("std::path::{Path, PathBuf}"));
        assert!(serialized.contains("- [ ] TODO"));
        assert!(serialized.contains("call(a, b)"));
        assert!(!serialized.contains("std\\:\\:"));
        assert!(!serialized.contains("\\[ \\]"));
    }

    #[test]
    fn serializer_escapes_only_unbalanced_value_parentheses() {
        let serialized = serialize_document(&[Node::file("notes.md", "literal ) and literal (")]);
        assert!(serialized.contains("literal \\) and literal \\("));

        let nodes = parse_document(&serialized).unwrap();
        assert_eq!(nodes[0].option("content"), Some("literal ) and literal ("));
    }

    #[test]
    fn parses_balanced_tag_brackets() {
        let nodes = parse_document("[Tag[inner]](name:value)\n").unwrap();
        assert_eq!(nodes[0].tag, "Tag[inner]");
    }

    #[test]
    fn serializer_escapes_only_unbalanced_tag_brackets() {
        let serialized = serialize_document(&[Node {
            tag: "Tag]open[".to_string(),
            options: Vec::new(),
            children: Vec::new(),
        }]);
        assert!(serialized.contains("[Tag\\]open\\[]"));

        let nodes = parse_document(&serialized).unwrap();
        assert_eq!(nodes[0].tag, "Tag]open[");
    }

    #[test]
    fn rejects_unknown_escape() {
        assert!(parse_document("[File](name:a)(content:\\n)").is_err());
    }

    #[test]
    fn encodes_and_decodes_folders() {
        let root = test_dir("naft-root");
        let out = test_dir("naft-out");
        fs::remove_dir_all(&out).unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("a.md"), "hello").unwrap();

        let nodes = encode_folders(std::slice::from_ref(&root)).unwrap();
        decode_to_base(&nodes, std::env::temp_dir()).unwrap_err();
        fs::create_dir(&out).unwrap();
        decode_to_base(&nodes, &out).unwrap();
        assert_eq!(
            fs::read_to_string(out.join(root.file_name().unwrap()).join("sub").join("a.md"))
                .unwrap(),
            "hello"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(out).unwrap();
    }

    #[test]
    fn encodes_non_utf8_files_as_base64() {
        let root = test_dir("naft-binary-root");
        let out = test_dir("naft-binary-out");
        fs::remove_dir_all(&out).unwrap();
        fs::write(root.join("bad.bin"), [0xff, 0xfe, 0x00]).unwrap();

        let nodes = encode_folders(std::slice::from_ref(&root)).unwrap();
        let serialized = serialize_document(&nodes);
        assert!(serialized.contains("content_base64://4A"));

        fs::create_dir(&out).unwrap();
        decode_to_base(&nodes, &out).unwrap();
        assert_eq!(
            fs::read(out.join(root.file_name().unwrap()).join("bad.bin")).unwrap(),
            vec![0xff, 0xfe, 0x00]
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(out).unwrap();
    }

    #[test]
    fn encode_skips_hidden_and_gitignored_paths_by_default() {
        let root = test_dir("naft-ignore-root");
        fs::write(root.join(".gitignore"), "ignored.txt\n*.tmp\n").unwrap();
        fs::write(root.join("visible.txt"), "visible").unwrap();
        fs::write(root.join(".hidden.txt"), "hidden").unwrap();
        fs::write(root.join("ignored.txt"), "ignored").unwrap();
        fs::write(root.join("scratch.tmp"), "ignored").unwrap();

        let nodes = encode_folders(std::slice::from_ref(&root)).unwrap();
        let serialized = serialize_document(&nodes);

        assert!(serialized.contains("visible.txt"));
        assert!(!serialized.contains(".hidden.txt"));
        assert!(!serialized.contains("ignored.txt"));
        assert!(!serialized.contains("scratch.tmp"));
        assert!(!serialized.contains(".gitignore"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encode_options_can_include_hidden_and_ignored_paths() {
        let root = test_dir("naft-include-root");
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join(".hidden.txt"), "hidden").unwrap();
        fs::write(root.join("ignored.txt"), "ignored").unwrap();

        let nodes = encode_folders_with_options(
            std::slice::from_ref(&root),
            &EncodeOptions {
                include_hidden: true,
                include_ignored: true,
                ..EncodeOptions::default()
            },
            |_| {},
        )
        .unwrap();
        let serialized = serialize_document(&nodes);

        assert!(serialized.contains(".hidden.txt"));
        assert!(serialized.contains("ignored.txt"));
        assert!(serialized.contains(".gitignore"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encode_honors_anchored_directory_gitignore_patterns() {
        let root = test_dir("anchored-ignore");
        fs::write(root.join(".gitignore"), "/target/\n").unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target").join("ignored.txt"), "ignored").unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::create_dir(root.join("nested").join("target")).unwrap();
        fs::write(root.join("nested").join("target").join("kept.txt"), "kept").unwrap();

        let default_nodes = encode_folders(std::slice::from_ref(&root)).unwrap();
        let default_text = serialize_document(&default_nodes);
        assert!(!default_text.contains("ignored.txt"));
        assert!(default_text.contains("kept.txt"));

        let included_nodes = encode_folders_with_options(
            std::slice::from_ref(&root),
            &EncodeOptions {
                include_ignored: true,
                ..EncodeOptions::default()
            },
            |_| {},
        )
        .unwrap();
        assert!(serialize_document(&included_nodes).contains("ignored.txt"));
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
