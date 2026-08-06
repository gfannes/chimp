use std::fs;
use std::path::PathBuf;
use std::process::Command;
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

fn test_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("chimp-{name}-{unique}"));
    fs::create_dir(&dir).unwrap();
    dir
}
