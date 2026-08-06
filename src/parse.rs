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
    pub assignee_exclusive: bool,
    pub bare_assignees: Vec<String>,
    pub empty_amp_paths: usize,
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
            && self.bare_assignees.is_empty()
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
        if other.assignee_exclusive {
            self.assignee = other.assignee.clone();
            self.assignee_exclusive = true;
        } else if self.assignee.is_none() {
            self.assignee = other.assignee.clone();
        }
        self.bare_assignees
            .extend(other.bare_assignees.iter().cloned());
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
#[allow(clippy::enum_variant_names)]
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
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "rb" | "rs" | "zig")
    )
}

fn source_comment_content(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let whitespace = line.len() - trimmed.len();
    let content = trimmed
        .strip_prefix("//")
        .or_else(|| trimmed.strip_prefix("#"))
        .or_else(|| trimmed.strip_prefix("/*"))
        .or_else(|| trimmed.strip_prefix("*"))?;
    let content = content.trim_start();
    if !content.starts_with('&') || !content.chars().nth(1).is_some_and(is_amp_start) {
        return None;
    }
    Some((whitespace + 1, content))
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

    let normalized_wikilinks = normalize_wikilink_references(line);
    for word in metadata_words(&normalized_wikilinks) {
        match word.as_str() {
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
            _ if is_empty_amp_path(&word) => md.empty_amp_paths += 1,
            _ if word.starts_with("&&") && word.len() > 2 => md.definitions.push(word.to_string()),
            _ if word.starts_with('&') && word.len() > 1 => parse_amp_metadata(&word, &mut md),
            _ if word.starts_with('@') && word.len() > 1 => {
                md.bare_assignees.push(word[1..].to_ascii_lowercase());
            }
            _ => {}
        }
    }

    md
}

fn is_empty_amp_path(word: &str) -> bool {
    word.starts_with('&')
        && word
            .trim_start_matches('&')
            .trim_start_matches('^')
            .trim_matches(':')
            .trim_matches('`')
            .is_empty()
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
                if output
                    .split_whitespace()
                    .next_back()
                    .is_some_and(|token| token.starts_with('&'))
                {
                    output.push(ch);
                    for next in chars.by_ref() {
                        output.push(next);
                        if next == '`' {
                            break;
                        }
                    }
                    continue;
                }
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
    let line = strip_wikilink_amp_metadata(line);
    let mut output = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch == '&' && chars.peek().is_some_and(|(_, next)| is_amp_start(*next)) {
            let mut quoted = false;
            while let Some((_, next)) = chars.peek().copied() {
                if next == '`' {
                    quoted = !quoted;
                    chars.next();
                } else if quoted || is_metadata_word_char(next) {
                    chars.next();
                } else {
                    break;
                }
            }
            if output.ends_with(' ') {
                while chars
                    .peek()
                    .is_some_and(|(_, next)| *next == ' ' || *next == '\t')
                {
                    chars.next();
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn normalize_wikilink_references(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;
    while let Some(start) = remaining.find("&[[") {
        let after_open = &remaining[start + 3..];
        let Some(end) = after_open.find("]]") else {
            break;
        };
        output.push_str(&remaining[..start]);
        let target = after_open[..end].trim();
        let path = target
            .split('/')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(":");
        if path.is_empty() {
            output.push_str("&[[");
            output.push_str(&after_open[..end]);
            output.push_str("]]");
        } else {
            output.push('&');
            output.push_str(&path);
        }
        remaining = &after_open[end + 2..];
    }
    output.push_str(remaining);
    output
}

fn strip_wikilink_amp_metadata(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;
    while let Some(start) = remaining.find("&[[") {
        let after_open = &remaining[start + 3..];
        let Some(end) = after_open.find("]]") else {
            break;
        };
        output.push_str(&remaining[..start]);
        remaining = &after_open[end + 2..];
        if output.ends_with(' ') {
            remaining = remaining.trim_start_matches([' ', '\t']);
        }
    }
    output.push_str(remaining);
    output
}

fn is_amp_start(ch: char) -> bool {
    ch == '&'
        || ch == '`'
        || ch == ':'
        || ch == '#'
        || ch == '^'
        || ch == '@'
        || ch == '?'
        || ch == '+'
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
        || ch == '+'
}

fn metadata_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        if ch == '`' && (quoted || current.starts_with('&')) {
            quoted = !quoted;
            current.push(ch);
        } else if quoted || is_metadata_word_char(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn parse_amp_metadata(word: &str, md: &mut Metadata) {
    let value = &word[1..];
    if let Some(date) = parse_date_metadata(value) {
        md.date = Some(date);
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
            md.assignee = Some(assignee.to_lowercase());
            md.assignee_exclusive = false;
        }
    } else if let Some(assignee) = value.strip_prefix("^@") {
        if !assignee.is_empty() {
            md.assignee = Some(assignee.to_lowercase());
            md.assignee_exclusive = true;
        }
    } else if let Some(wbs) = value.strip_prefix('?') {
        if !wbs.is_empty() {
            md.wbs.push(wbs.to_ascii_lowercase());
        }
    } else {
        md.references.push(word.to_string());
    }
}

fn parse_date_metadata(value: &str) -> Option<String> {
    let (date, offset) = value
        .split_once('+')
        .map_or((value, None), |(date, offset)| (date, Some(offset)));
    if date.len() != 8 || !date.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let year = date[0..4].parse::<i32>().ok()?;
    let month = date[4..6].parse::<u32>().ok()?;
    let day = date[6..8].parse::<u32>().ok()?;
    if year < 1 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let (year, month, day) = match offset {
        None => (year, month, day),
        Some(offset) => {
            let months = offset.strip_suffix('m')?.parse::<i32>().ok()?;
            let absolute_month = year
                .checked_mul(12)?
                .checked_add(month as i32 - 1)?
                .checked_add(months)?;
            if absolute_month < 12 {
                return None;
            }
            let year = absolute_month.div_euclid(12);
            let month = absolute_month.rem_euclid(12) as u32 + 1;
            (year, month, day.min(days_in_month(year, month)))
        }
    };
    Some(format!("{year:04}{month:02}{day:02}"))
}

pub(crate) fn date_in_path_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut dates = Vec::new();
    for start in 0..bytes.len() {
        if start > 0 && bytes[start - 1].is_ascii_digit() {
            continue;
        }
        for len in [8, 10] {
            let Some(candidate) = value.get(start..start + len) else {
                continue;
            };
            if start + len < bytes.len() && bytes[start + len].is_ascii_digit() {
                continue;
            }
            let normalized = if len == 10
                && candidate.as_bytes()[4] == b'-'
                && candidate.as_bytes()[7] == b'-'
            {
                format!("{}{}{}", &candidate[..4], &candidate[5..7], &candidate[8..])
            } else if len == 8 {
                candidate.to_string()
            } else {
                continue;
            };
            if let Some(date) = parse_date_metadata(&normalized) {
                dates.push(date);
            }
        }
    }
    dates.into_iter().min()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
    fn applies_month_offset_to_date_metadata() {
        assert_eq!(
            extract_metadata("&20260806+1m").date.as_deref(),
            Some("20260906")
        );
        assert_eq!(
            extract_metadata("&20260131+1m").date.as_deref(),
            Some("20260228")
        );
        assert_eq!(
            extract_metadata("&20240229+12m").date.as_deref(),
            Some("20250228")
        );
    }

    #[test]
    fn rejects_invalid_date_metadata_as_a_reference() {
        let metadata = extract_metadata("&20260230");
        assert!(metadata.date.is_none());
        assert_eq!(metadata.references, vec!["&20260230"]);
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
    fn extracts_exclusive_assignee_metadata() {
        let md = extract_metadata("- [ ] task &^@geert");
        assert_eq!(md.assignee.as_deref(), Some("geert"));
        assert!(md.assignee_exclusive);
    }

    #[test]
    fn collects_bare_assignee_candidates() {
        let md = extract_metadata("- [ ] ask @Geert and @alice");
        assert_eq!(md.bare_assignees, vec!["geert", "alice"]);
        assert!(md.assignee.is_none());
    }

    #[test]
    fn empty_amp_paths_are_counted_but_omitted() {
        let md = extract_metadata("- [ ] task & && &: &^:");
        assert_eq!(md.empty_amp_paths, 4);
        assert!(md.references.is_empty());
        assert!(md.definitions.is_empty());
    }

    #[test]
    fn extracts_dates_from_path_components() {
        assert_eq!(
            date_in_path_component("meeting-2026-08-06-notes.md").as_deref(),
            Some("20260806")
        );
        assert_eq!(
            date_in_path_component("backup_20260807.txt").as_deref(),
            Some("20260807")
        );
        assert!(date_in_path_component("invalid-2026-02-30.md").is_none());
    }

    #[test]
    fn wikilink_references_are_normalized_in_source_order() {
        let md = extract_metadata("- [ ] &before &[[a/b]] &after");
        assert_eq!(md.references, vec!["&before", "&a:b", "&after"]);
    }

    #[test]
    fn strips_complete_wikilink_metadata() {
        assert_eq!(
            strip_amp_metadata("- [ ] Read &[[a/b]] today\n"),
            "- [ ] Read today\n"
        );
    }

    #[test]
    fn backticks_quote_amp_path_spaces_and_colons() {
        let md = extract_metadata("- [ ] &root:`part one:two`:leaf work");
        assert_eq!(md.references, vec!["&root:`part one:two`:leaf"]);
    }

    #[test]
    fn strips_amp_paths_with_backtick_quoted_segments() {
        assert_eq!(
            strip_amp_metadata("- [ ] Read &root:`part one:two`:leaf today\n"),
            "- [ ] Read today\n"
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
    fn recognizes_source_comments_that_begin_with_amp_path() {
        assert_eq!(
            source_comment_content("  // &task TODO x").unwrap().1,
            "&task TODO x"
        );
        assert!(source_comment_content("  // TODO &task x").is_none());
        assert_eq!(
            source_comment_content("// &task TODO x").unwrap().1,
            "&task TODO x"
        );
    }
}
