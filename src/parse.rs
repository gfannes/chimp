use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub definitions: Vec<String>,
    pub references: Vec<String>,
    pub status: Option<crate::Status>,
    pub checkbox: Option<bool>,
    pub date: Option<String>,
    pub order: Option<crate::OrderMetadata>,
    pub assignee: Option<String>,
    pub wbs: Vec<String>,
}

impl Metadata {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
            && self.references.is_empty()
            && self.status.is_none()
            && self.checkbox.is_none()
            && self.date.is_none()
            && self.order.is_none()
            && self.assignee.is_none()
            && self.wbs.is_empty()
    }

    pub fn is_chore_marker(&self) -> bool {
        !self.references.is_empty()
            || !self.definitions.is_empty()
            || self.status.is_some()
            || self.checkbox.is_some()
            || !self.wbs.is_empty()
    }

    pub fn merge(&mut self, other: &Metadata) {
        self.definitions.extend(other.definitions.iter().cloned());
        self.references.extend(other.references.iter().cloned());
        self.status = self.status.or(other.status);
        self.checkbox = self.checkbox.or(other.checkbox);
        self.date = self.date.clone().or_else(|| other.date.clone());
        self.order = self.order.or(other.order);
        self.assignee = self.assignee.clone().or_else(|| other.assignee.clone());
        self.wbs.extend(other.wbs.iter().cloned());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContentLine<'a> {
    pub text: &'a str,
    pub column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownState {
    fenced_code: Option<String>,
    block_math: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLine {
    pub text: String,
    pub issues: Vec<MarkdownIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownIssue {
    pub kind: MarkdownIssueKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownIssueKind {
    UnclosedInlineCode,
    UnclosedInlineFormula,
    UnclosedFencedCodeBlock,
    UnclosedFormulaBlock,
}

pub fn content_line<'a>(line: &'a str, is_markdown: bool, path: &Path) -> Option<ContentLine<'a>> {
    if is_markdown {
        return Some(ContentLine {
            text: line,
            column: 1,
        });
    }
    if !is_supported_source(path) {
        return None;
    }
    source_comment_content(line).map(|(column, text)| ContentLine { text, column })
}

#[allow(dead_code)]
pub fn markdown_visible_line(line: &str, state: &mut MarkdownState) -> Option<String> {
    markdown_visible_line_with_issues(line, state).map(|line| line.text)
}

pub fn markdown_visible_line_with_issues(
    line: &str,
    state: &mut MarkdownState,
) -> Option<MarkdownLine> {
    let trimmed = line.trim_start();
    if let Some(marker) = state.fenced_code.as_deref() {
        if trimmed.starts_with(marker) {
            state.fenced_code = None;
        }
        return None;
    }
    if state.block_math {
        if is_block_math_delimiter(trimmed) {
            state.block_math = false;
        }
        return None;
    }
    if let Some(marker) = fence_marker(trimmed) {
        state.fenced_code = Some(marker.to_string());
        return None;
    }
    if is_block_math_delimiter(trimmed) {
        state.block_math = true;
        return None;
    }
    Some(mask_inline_code_and_math(line))
}

impl MarkdownState {
    pub fn finish_issues(&self) -> Vec<MarkdownIssue> {
        let mut issues = Vec::new();
        if self.fenced_code.is_some() {
            issues.push(MarkdownIssue {
                kind: MarkdownIssueKind::UnclosedFencedCodeBlock,
                message: "unclosed Markdown fenced code block".to_string(),
            });
        }
        if self.block_math {
            issues.push(MarkdownIssue {
                kind: MarkdownIssueKind::UnclosedFormulaBlock,
                message: "unclosed Markdown formula block".to_string(),
            });
        }
        issues
    }
}

fn is_supported_source(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "rb" | "rs" | "zig") => true,
        _ => false,
    }
}

fn source_comment_content(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    if trimmed.len() == line.len() {
        return None;
    }
    let whitespace = line.len() - trimmed.len();
    let content = trimmed
        .strip_prefix("//")
        .or_else(|| trimmed.strip_prefix("#"))
        .or_else(|| trimmed.strip_prefix("/*"))
        .or_else(|| trimmed.strip_prefix("*"))?;
    Some((whitespace + 1, content.trim_start()))
}

pub fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes)
    } else {
        None
    }
}

pub fn markdown_item_indent(line: &str) -> Option<usize> {
    let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || numbered_item(trimmed).is_some() {
        Some(indent)
    } else {
        None
    }
}

pub fn extract_metadata(line: &str) -> Metadata {
    let mut md = Metadata::default();
    let without_bullet = strip_markdown_bullet(line.trim_start());

    if let Some(status) = checkbox_status(without_bullet) {
        md.checkbox = Some(matches!(status, crate::Status::Done));
        md.status = Some(status);
    }

    for word in line.split(|ch: char| !is_metadata_word_char(ch)) {
        match word {
            "TODO" => md.status = Some(crate::Status::Todo),
            "GO" => md.status = Some(crate::Status::Go),
            "DONE" => md.status = Some(crate::Status::Done),
            "QUESTION" => md.status = Some(crate::Status::Question),
            "INFO" => md.status = Some(crate::Status::Info),
            "WIP" => md.status = Some(crate::Status::Wip),
            "BLOCKED" => md.status = Some(crate::Status::Blocked),
            "FORWARD" => md.status = Some(crate::Status::Forward),
            "PLANNED" => md.status = Some(crate::Status::Planned),
            "CANCELED" | "CANCELLED" => md.status = Some(crate::Status::Canceled),
            "ASSIGNED" => md.status = Some(crate::Status::Assigned),
            _ if word.starts_with("&&") && word.len() > 2 => md.definitions.push(word.to_string()),
            _ if word.starts_with('&') && word.len() > 1 => parse_amp_metadata(word, &mut md),
            _ => {}
        }
    }

    md
}

fn checkbox_status(line: &str) -> Option<crate::Status> {
    let marker = line.strip_prefix('[')?.chars().next()?;
    if !line.get(2..)?.starts_with(']') {
        return None;
    }
    match marker {
        ' ' => Some(crate::Status::Todo),
        '*' => Some(crate::Status::Go),
        '/' => Some(crate::Status::Wip),
        'x' | 'X' => Some(crate::Status::Done),
        '?' => Some(crate::Status::Question),
        'i' | 'I' => Some(crate::Status::Info),
        '!' => Some(crate::Status::Blocked),
        '>' => Some(crate::Status::Forward),
        '<' => Some(crate::Status::Planned),
        '-' => Some(crate::Status::Canceled),
        '~' => Some(crate::Status::Assigned),
        _ => None,
    }
}

pub fn strip_amp_metadata(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        stripped.push_str(&strip_amp_metadata_line(line));
    }
    if !text.ends_with('\n') && text.is_empty() {
        stripped.clear();
    }
    stripped
}

fn fence_marker(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn is_block_math_delimiter(trimmed: &str) -> bool {
    trimmed == "$$" || trimmed.starts_with("$$ ")
}

fn mask_inline_code_and_math(line: &str) -> MarkdownLine {
    let mut output = String::with_capacity(line.len());
    let mut issues = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                output.push(' ');
                let mut closed = false;
                for next in chars.by_ref() {
                    output.push(' ');
                    if next == '`' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    issues.push(MarkdownIssue {
                        kind: MarkdownIssueKind::UnclosedInlineCode,
                        message: "unclosed Markdown inline code span".to_string(),
                    });
                }
            }
            '$' if chars
                .peek()
                .is_some_and(|next| *next != '$' && *next != ' ') =>
            {
                output.push(' ');
                let mut closed = false;
                for next in chars.by_ref() {
                    output.push(' ');
                    if next == '$' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    issues.push(MarkdownIssue {
                        kind: MarkdownIssueKind::UnclosedInlineFormula,
                        message: "unclosed Markdown inline formula".to_string(),
                    });
                }
            }
            _ => output.push(ch),
        }
    }
    MarkdownLine {
        text: output,
        issues,
    }
}

fn strip_amp_metadata_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '&' && chars.peek().is_some_and(|(_, next)| is_amp_start(*next)) {
            while let Some((_, next)) = chars.peek().copied() {
                if !is_metadata_word_char(next) {
                    break;
                }
                chars.next();
            }
            if idx > 0 && output.ends_with(' ') {
                while let Some((_, next)) = chars.peek().copied() {
                    if next == ' ' || next == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn is_amp_start(ch: char) -> bool {
    ch == '&'
        || ch == ':'
        || ch == '#'
        || ch == '^'
        || ch == '@'
        || ch == '?'
        || ch.is_ascii_alphanumeric()
        || ch == '_'
}

fn is_metadata_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || ch == '_'
        || ch == '&'
        || ch == ':'
        || ch == '#'
        || ch == '^'
        || ch == '@'
        || ch == '?'
}

fn parse_amp_metadata(word: &str, md: &mut Metadata) {
    let value = &word[1..];
    if value.len() == 8 && value.chars().all(|ch| ch.is_ascii_digit()) {
        md.date = Some(value.to_string());
    } else if let Some(order) = value
        .strip_prefix("#")
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        md.order = Some(crate::OrderMetadata {
            value: order,
            exclusive: false,
        });
    } else if let Some(order) = value
        .strip_prefix("^#")
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        md.order = Some(crate::OrderMetadata {
            value: order,
            exclusive: true,
        });
    } else if let Some(assignee) = value.strip_prefix('@') {
        if !assignee.is_empty() {
            md.assignee = Some(assignee.to_string());
        }
    } else if let Some(wbs) = value.strip_prefix('?') {
        if !wbs.is_empty() {
            md.wbs.push(wbs.to_ascii_lowercase());
        }
    } else {
        md.references.push(word.to_string());
    }
}

fn strip_markdown_bullet(mut line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return rest.trim_start();
    }
    if let Some(width) = numbered_item(line) {
        line = &line[width..];
        return line.trim_start();
    }
    line
}

fn numbered_item(line: &str) -> Option<usize> {
    let digits = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        Some(digits + 2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_core_metadata() {
        let md = extract_metadata("- [ ] TODO &proj:scan &20260805 &#12 &@geert");
        assert_eq!(md.checkbox, Some(false));
        assert_eq!(md.status, Some(crate::Status::Todo));
        assert_eq!(md.references, vec!["&proj:scan"]);
        assert_eq!(md.date.as_deref(), Some("20260805"));
        assert_eq!(
            md.order,
            Some(crate::OrderMetadata {
                value: 12,
                exclusive: false
            })
        );
        assert_eq!(md.assignee.as_deref(), Some("geert"));
    }

    #[test]
    fn extracts_exclusive_order_metadata() {
        let md = extract_metadata("- [ ] TODO &^#12");
        assert_eq!(
            md.order,
            Some(crate::OrderMetadata {
                value: 12,
                exclusive: true
            })
        );
    }

    #[test]
    fn extracts_wbs_metadata() {
        let md = extract_metadata("- [ ] &&:chimp:docs &?project");
        assert_eq!(md.definitions, vec!["&&:chimp:docs"]);
        assert_eq!(md.wbs, vec!["project"]);
    }

    #[test]
    fn unchecked_checkbox_implies_todo() {
        let md = extract_metadata("- [ ] &proj:scan");
        assert_eq!(md.checkbox, Some(false));
        assert_eq!(md.status, Some(crate::Status::Todo));
    }

    #[test]
    fn canceled_checkbox_implies_canceled() {
        let md = extract_metadata("- [-] &proj:scan");
        assert_eq!(md.checkbox, Some(false));
        assert_eq!(md.status, Some(crate::Status::Canceled));
    }

    #[test]
    fn extracts_all_checkbox_statuses() {
        let cases = [
            ("[ ]", crate::Status::Todo),
            ("[*]", crate::Status::Go),
            ("[/]", crate::Status::Wip),
            ("[x]", crate::Status::Done),
            ("[?]", crate::Status::Question),
            ("[i]", crate::Status::Info),
            ("[!]", crate::Status::Blocked),
            ("[>]", crate::Status::Forward),
            ("[<]", crate::Status::Planned),
            ("[-]", crate::Status::Canceled),
            ("[~]", crate::Status::Assigned),
        ];

        for (marker, status) in cases {
            let md = extract_metadata(&format!("- {marker} &task"));
            assert_eq!(md.status, Some(status));
        }
    }

    #[test]
    fn strip_amp_metadata_keeps_non_metadata_amp_text() {
        assert_eq!(
            strip_amp_metadata("Use `&.md` and TODO &real:tag &?project.\n"),
            "Use `&.md` and TODO .\n"
        );
    }

    #[test]
    fn markdown_visible_line_masks_inline_code_and_formula() {
        let mut state = MarkdownState::default();
        let visible = markdown_visible_line(
            "Keep `TODO &code` and $WIP &math$ visible TODO &real",
            &mut state,
        )
        .unwrap();
        let md = extract_metadata(&visible);
        assert_eq!(md.status, Some(crate::Status::Todo));
        assert_eq!(md.references, vec!["&real"]);
    }

    #[test]
    fn markdown_visible_line_skips_code_and_math_blocks() {
        let mut state = MarkdownState::default();
        assert!(markdown_visible_line("```rust", &mut state).is_none());
        assert!(markdown_visible_line("TODO &code", &mut state).is_none());
        assert!(markdown_visible_line("```", &mut state).is_none());
        assert!(markdown_visible_line("TODO &real", &mut state).is_some());

        assert!(markdown_visible_line("$$", &mut state).is_none());
        assert!(markdown_visible_line("TODO &math", &mut state).is_none());
        assert!(markdown_visible_line("$$", &mut state).is_none());
        assert!(markdown_visible_line("TODO &after", &mut state).is_some());
    }

    #[test]
    fn extracts_definitions() {
        let md = extract_metadata("&&:chimp:parser DONE");
        assert_eq!(md.definitions, vec!["&&:chimp:parser"]);
        assert_eq!(md.status, Some(crate::Status::Done));
    }

    #[test]
    fn recognizes_indented_source_comments_only() {
        assert_eq!(source_comment_content("  // TODO x").unwrap().1, "TODO x");
        assert!(source_comment_content("// TODO x").is_none());
    }
}
