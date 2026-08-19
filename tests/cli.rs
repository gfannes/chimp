use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn chores_details_respects_nocolor_and_ordering() {
    let dir = test_dir("cli-chores");
    let home = test_dir("cli-chores-home");
    fs::write(
        dir.join("notes.md"),
        [
            "- [ ] TODO &missing loose",
            "# Later &&:later &#9",
            "- [ ] TODO &later sequenced",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("--details")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("\x1b["));
    assert!(stdout.contains("details: computed_order=-"));
    assert!(stdout.contains("details: computed_order=9"));
    assert!(stdout.find("loose").unwrap() < stdout.find("sequenced").unwrap());
    assert!(stdout.contains("## `"));
    assert!(stdout.contains("- [ ] TODO &missing loose  [1:1 order=-"));
    assert!(stdout.contains("- [ ] TODO &later sequenced  [3:1 order=9"));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_limit_reports_first_sorted_chores() {
    let dir = test_dir("cli-chores-limit");
    let home = test_dir("cli-chores-limit-home");
    fs::write(
        dir.join("notes.md"),
        ["- [ ] TODO &missing loose", "- [ ] TODO &other second", ""].join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-n")
        .arg("1")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("loose"));
    assert!(!stdout.contains("1:1 ["));
    assert!(!stdout.contains("order="));
    assert!(!stdout.contains("second"));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_text_prefix_filters_only_raw_chore_text() {
    let dir = test_dir("cli-chores-raw-text");
    let home = test_dir("cli-chores-raw-text-home");
    fs::write(
        dir.join("notes.md"),
        [
            "# Metadata match &&:needle",
            "- [ ] unrelated wording",
            "- [ ] contains NEEDLE here",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-C")
        .arg(&dir)
        .arg("text:needle")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("contains NEEDLE here"));
    assert!(!stdout.contains("unrelated wording"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_report_only_active_statuses() {
    let dir = test_dir("cli-chores-status");
    let home = test_dir("cli-chores-status-home");
    fs::write(
        dir.join("notes.md"),
        [
            "- [ ] &todo visible-todo",
            "- [*] &go visible-go",
            "- [/] &wip visible-wip",
            "- [?] &question visible-question",
            "- [!] &blocked visible-blocked",
            "- [x] &done hidden-done",
            "- [i] &info hidden-info",
            "- [>] &forward hidden-forward",
            "- [<] &planned hidden-planned",
            "- [-] &canceled hidden-canceled",
            "- [~] &assigned hidden-assigned",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("visible-todo"));
    assert!(stdout.contains("visible-go"));
    assert!(stdout.contains("visible-wip"));
    assert!(stdout.contains("visible-question"));
    assert!(stdout.contains("visible-blocked"));
    assert!(!stdout.contains("hidden-done"));
    assert!(!stdout.contains("hidden-info"));
    assert!(!stdout.contains("hidden-forward"));
    assert!(!stdout.contains("hidden-planned"));
    assert!(!stdout.contains("hidden-canceled"));
    assert!(!stdout.contains("hidden-assigned"));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_hide_items_whose_earliest_date_is_in_the_future() {
    let dir = test_dir("cli-chores-date");
    let home = test_dir("cli-chores-date-home");
    fs::write(
        dir.join("notes.md"),
        [
            "# Notes",
            "# Available &&:available &20000101",
            "- [ ] &available visible-related-past &29990101",
            "# Future &&:future &29990101",
            "- [ ] &future hidden-related-future",
            "# General",
            "- [ ] visible-direct-past &20000101",
            "- [ ] hidden-direct-future &29990101",
            "- [ ] visible-undated",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("visible-related-past"));
    assert!(stdout.contains("visible-direct-past"));
    assert!(stdout.contains("visible-undated"));
    assert!(!stdout.contains("hidden-related-future"));
    assert!(!stdout.contains("hidden-direct-future"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_help_documents_query_syntax() {
    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("chores")
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("QUERY:"));
    assert!(stdout.contains("text:TERM"));
    assert!(stdout.contains("@NAME"));
    assert!(stdout.contains("use AND"));
}

#[test]
fn chores_are_ordered_globally_across_files() {
    let dir = test_dir("cli-chores-global");
    let home = test_dir("cli-chores-global-home");
    fs::write(
        dir.join("a.md"),
        [
            "# High &&:a:high &#9",
            "- [ ] TODO &a:high high-a",
            "# Low &&:a:low &#1",
            "- [ ] TODO &a:low low-a",
            "",
        ]
        .join("\n"),
    )
    .unwrap();
    fs::write(
        dir.join("b.md"),
        [
            "# Mid &&:b:mid &#5",
            "- [ ] TODO &b:mid mid-b",
            "# Low &&:b:low &#1",
            "- [ ] TODO &b:low low-b",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.find("high-a").unwrap() < stdout.find("mid-b").unwrap());
    assert!(stdout.find("mid-b").unwrap() < stdout.find("low-a").unwrap());
    assert!(stdout.find("low-a").unwrap() < stdout.find("low-b").unwrap());
    assert_eq!(stdout.matches("a.md").count(), 2);

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_use_default_assignee_from_config_for_filters() {
    let dir = test_dir("cli-chores-default-assignee");
    let home = test_dir("cli-chores-default-assignee-home");
    fs::write(
        dir.join("chimp.toml"),
        r#"
default_assignee = "fallback"
root = "."
"#,
    )
    .unwrap();
    fs::write(
        dir.join("notes.md"),
        [
            "- [ ] TODO &unassigned loose",
            "- [ ] TODO &assigned &@alice owned",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let fallback = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("@fallback")
        .output()
        .unwrap();

    assert!(fallback.status.success());
    let stdout = String::from_utf8(fallback.stdout).unwrap();
    assert!(stdout.contains("loose"));
    assert!(!stdout.contains("owned"));

    let either = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("@fallback")
        .arg("@alice")
        .output()
        .unwrap();

    assert!(either.status.success());
    let stdout = String::from_utf8(either.stdout).unwrap();
    assert!(stdout.contains("loose"));
    assert!(stdout.contains("owned"));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn exclusive_assignee_breaks_inheritance() {
    let dir = test_dir("cli-exclusive-assignee");
    let home = test_dir("cli-exclusive-assignee-home");
    fs::write(
        dir.join("notes.md"),
        [
            "# Alice &&@alice",
            "# Bob &&@bob",
            "# Geert &&@geert",
            "# Carol &&@carol",
            "# Root &&:root &@alice",
            "## Team &&team &@bob",
            "### Focus &&focus &^@geert",
            "- [ ] exclusive-task",
            "#### Detail &&detail &@carol",
            "- [ ] descendant-task",
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let exclusive = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("--details")
        .arg("-C")
        .arg(&dir)
        .arg("exclusive-task")
        .output()
        .unwrap();
    assert!(exclusive.status.success());
    let stdout = String::from_utf8(exclusive.stdout).unwrap();
    assert!(stdout.contains("assignee=geert"));
    assert!(!stdout.contains("assignee=alice"));
    assert!(!stdout.contains("assignee=bob"));

    let descendant = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("--details")
        .arg("-C")
        .arg(&dir)
        .arg("descendant-task")
        .output()
        .unwrap();
    assert!(descendant.status.success());
    let stdout = String::from_utf8(descendant.stdout).unwrap();
    assert!(stdout.contains("assignee=geert,carol"));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn config_displays_effective_merged_configuration() {
    let dir = test_dir("cli-config");
    let home = test_dir("cli-config-home");
    let global_dir = home.join(".config/chimp");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("config.toml"),
        "default_assignee = \"global\"\nroot = \"global-notes\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("chimp.toml"),
        "default_assignee = \"local\"\n[[grove]]\npath = \"docs\"\nextensions = [\"md\"]\nmax_filesize = 2048\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .arg("config")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("# Chimp configuration"));
    assert!(stdout.contains("Default assignee: `local`"));
    assert!(stdout.contains("global-notes"));
    assert!(stdout.contains("docs"));
    assert!(stdout.contains("extensions: `md`"));
    assert!(stdout.contains("max filesize: `2048`"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn config_reports_config_toml_parse_errors_with_location() {
    let dir = test_dir("cli-config-error");
    let home = test_dir("cli-config-error-home");
    let global_dir = home.join(".config/chimp");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("config.toml"),
        "[[grove]]\npath = \"notes\"\nmax_filesize = huge\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .arg("config")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("config.toml"));
    assert!(stderr.contains("line 3"));
    assert!(stderr.contains("max_filesize"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn check_edit_opens_first_location_per_file_and_honors_limit() {
    let dir = test_dir("cli-check-edit");
    let home = test_dir("cli-check-edit-home");
    fs::write(
        dir.join("chimp.toml"),
        "editor = \"/bin/echo\"\nroot = \".\"\n",
    )
    .unwrap();
    fs::write(dir.join("a.md"), "- [ ] &missing-a first\n").unwrap();
    fs::write(dir.join("b.md"), "- [ ] &missing-b second\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .env("EDITOR", "/bin/false")
        .arg("--nocolor")
        .arg("check")
        .arg("-e")
        .arg("-n")
        .arg("1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().next_back().unwrap(),
        format!("{}:1:1", dir.join("a.md").display())
    );
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn chores_edit_opens_only_first_reported_item_for_each_file() {
    let dir = test_dir("cli-chores-edit");
    let home = test_dir("cli-chores-edit-home");
    fs::write(
        dir.join("chimp.toml"),
        "editor = \"/bin/echo {file} {line} {column}\"\nroot = \".\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("notes.md"),
        "- [ ] first-task\n- [ ] second-task\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .current_dir(&dir)
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("chores")
        .arg("-e")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().next_back().unwrap(),
        format!("{} 1 1", dir.join("notes.md").display())
    );
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn naft_cli_encodes_and_decodes() {
    let source = test_dir("cli-naft-source");
    let base = test_dir("cli-naft-base");
    let naft = base.join("fixture.naft");
    fs::create_dir(source.join("sub")).unwrap();
    fs::write(source.join("sub").join("a.md"), "hello (world) and )").unwrap();

    let encode = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("encode")
        .arg(&naft)
        .arg(&source)
        .output()
        .unwrap();
    assert!(encode.status.success());
    assert!(
        fs::read_to_string(&naft)
            .unwrap()
            .contains("hello (world) and \\)")
    );

    let decode = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("decode")
        .arg(&naft)
        .arg(&base)
        .output()
        .unwrap();
    assert!(decode.status.success());
    assert_eq!(
        fs::read_to_string(
            base.join(source.file_name().unwrap())
                .join("sub")
                .join("a.md")
        )
        .unwrap(),
        "hello (world) and )"
    );

    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn naft_cli_round_trips_non_utf8_content() {
    let source = test_dir("cli-naft-binary");
    let base = test_dir("cli-naft-binary-base");
    let naft = source.join("fixture.naft");
    let bad = source.join("bad.bin");
    fs::write(&bad, [0xff, 0xfe]).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("encode")
        .arg(&naft)
        .arg(&source)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        fs::read_to_string(&naft)
            .unwrap()
            .contains("content_base64://4=")
    );

    let decode = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("decode")
        .arg(&naft)
        .arg(&base)
        .output()
        .unwrap();
    assert!(decode.status.success());
    assert_eq!(
        fs::read(base.join(source.file_name().unwrap()).join("bad.bin")).unwrap(),
        vec![0xff, 0xfe]
    );

    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn naft_encode_verbose_three_reports_processed_paths() {
    let source = test_dir("cli-naft-verbose");
    let naft = source.join("fixture.naft");
    let file = source.join("a.md");
    fs::write(&file, "hello").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("-V")
        .arg("3")
        .arg("naft")
        .arg("encode")
        .arg(&naft)
        .arg(&source)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("processing"));
    assert!(stderr.contains("a.md"));

    fs::remove_dir_all(source).unwrap();
}

#[test]
fn check_reports_definition_and_assignee_validation_as_markdown() {
    let dir = test_dir("cli-check-validation");
    let home = test_dir("cli-check-validation-home");
    fs::write(
        dir.join("notes.md"),
        "# One &&:duplicate\n# Two &&:duplicate\n- [ ] &@missing task\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("check")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("**AmbiguousDefinition**"));
    assert!(stdout.contains("**UnresolvedAssignee**"));
    assert!(stdout.lines().next().unwrap().starts_with("- `"));
    assert!(stdout.contains("Issues: 3"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn check_reports_empty_amp_paths() {
    let dir = test_dir("cli-check-empty-amp");
    let home = test_dir("cli-check-empty-amp-home");
    fs::write(dir.join("notes.md"), "- [ ] keep &\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("--nocolor")
        .arg("check")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("**EmptyAmpPath**"));
    assert!(stdout.contains("empty AmpPath is not allowed and was omitted"));
    assert!(stdout.contains("Issues: 1"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn lsp_command_runs_framed_initialize_and_shutdown_session() {
    let dir = test_dir("cli-lsp");
    let home = test_dir("cli-lsp-home");
    fs::write(dir.join("notes.md"), "# Alpha &&:alpha\n").unwrap();
    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ];
    let input = messages
        .iter()
        .map(|body| format!("Content-Length: {}\r\n\r\n{body}", body.len()))
        .collect::<String>();

    let mut child = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("lsp")
        .arg("-C")
        .arg(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Content-Length: "));
    assert!(stdout.contains("\"positionEncoding\":\"utf-16\""));
    assert!(stdout.contains("\"id\":2"));
    assert!(stdout.contains("\"result\":null"));
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn verbosity_zero_suppresses_errors_and_three_reports_grove_paths() {
    let missing = test_dir("cli-verbose-missing").join("does-not-exist");
    let quiet = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("-V")
        .arg("0")
        .arg("scan")
        .arg(&missing)
        .output()
        .unwrap();
    assert!(!quiet.status.success());
    assert!(quiet.stderr.is_empty());

    let dir = test_dir("cli-verbose-scan");
    fs::write(dir.join("notes.md"), "- [ ] task\n").unwrap();
    let detailed = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("-V")
        .arg("3")
        .arg("scan")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(detailed.status.success());
    let stderr = String::from_utf8(detailed.stderr).unwrap();
    assert!(stderr.contains("processing"));
    assert!(stderr.contains("notes.md"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn naft_encode_skips_hidden_and_ignored_unless_flags_are_set() {
    let source = test_dir("cli-naft-filter");
    let default_naft = source.join("default.naft");
    let full_naft = source.join("full.naft");
    fs::write(source.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(source.join("visible.txt"), "visible").unwrap();
    fs::write(source.join(".hidden.txt"), "hidden").unwrap();
    fs::write(source.join("ignored.txt"), "ignored").unwrap();

    let default_output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("encode")
        .arg(&default_naft)
        .arg(&source)
        .output()
        .unwrap();
    assert!(default_output.status.success());
    let default_text = fs::read_to_string(&default_naft).unwrap();
    assert!(default_text.contains("visible.txt"));
    assert!(!default_text.contains(".hidden.txt"));
    assert!(!default_text.contains("ignored.txt"));

    let full_output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .arg("naft")
        .arg("encode")
        .arg(&full_naft)
        .arg("-u")
        .arg("-U")
        .arg(&source)
        .output()
        .unwrap();
    assert!(full_output.status.success());
    let full_text = fs::read_to_string(&full_naft).unwrap();
    assert!(full_text.contains(".hidden.txt"));
    assert!(full_text.contains("ignored.txt"));
    assert!(full_text.contains(".gitignore"));

    fs::remove_dir_all(source).unwrap();
}

#[test]
fn chores_measure_reports_phase_timings_to_stderr() {
    let dir = test_dir("cli-chores-measure");
    let home = test_dir("cli-chores-measure-home");
    fs::write(dir.join("notes.md"), "- [ ] TODO &work measured\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_chimp"))
        .env("HOME", &home)
        .arg("chores")
        .arg("--measure")
        .arg("-C")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("measured"));
    for phase in [
        "configuration:",
        "scanner total (file discovery and reading):",
        ".gitignore handling:",
        "directory entry enumeration:",
        "directory entry sorting:",
        "file metadata checks:",
        "file reads:",
        "UTF-8 conversion:",
        "other scanner work:",
        "scanner counts:",
        "parsing and validation:",
        "relationship resolution:",
        "Chore filtering and sorting:",
        "output:",
        "total:",
    ] {
        assert!(stderr.contains(phase), "missing {phase} in {stderr}");
    }

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(home).unwrap();
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
