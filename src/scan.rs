use std::fs;
use std::path::Path;

use crate::{Config, FileId, GroveConfig, Result, SourceFile};

pub fn load_files(config: &Config) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    for (grove, grove_config) in config.groves.iter().enumerate() {
        let root = grove_config.root.canonicalize()?;
        walk_root(grove, grove_config, &root, &root, Vec::new(), &mut files)?;
    }
    Ok(files)
}

fn walk_root(
    grove: usize,
    grove_config: &GroveConfig,
    root: &Path,
    dir: &Path,
    inherited_ignore: Vec<IgnorePattern>,
    files: &mut Vec<SourceFile>,
) -> Result<()> {
    let mut ignore = inherited_ignore;
    ignore.extend(read_gitignore(dir)?);

    let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') || ignored(root, &path, name, &ignore) {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_root(grove, grove_config, root, &path, ignore.clone(), files)?;
        } else if file_type.is_file() && is_supported_file(&path, grove_config) {
            if grove_config
                .max_filesize
                .is_some_and(|max| entry.metadata().is_ok_and(|metadata| metadata.len() > max))
            {
                continue;
            }
            let bytes = fs::read(&path)?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let id = FileId(files.len());
            files.push(SourceFile {
                id,
                grove,
                root: root.to_path_buf(),
                path,
                bytes,
                text,
            });
        }
    }

    Ok(())
}

fn is_supported_file(path: &Path, grove: &GroveConfig) -> bool {
    if path.file_name().and_then(|name| name.to_str()) == Some("&.md") {
        return true;
    }
    if !grove.extensions.is_empty() {
        let Some(file_ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };
        return grove
            .extensions
            .iter()
            .any(|ext| file_ext.eq_ignore_ascii_case(ext.trim_start_matches('.')));
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "rb" | "rs" | "zig")
    )
}

pub fn write_file_exact(file: &SourceFile, destination: impl AsRef<Path>) -> Result<()> {
    fs::write(destination.as_ref(), &file.bytes)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct IgnorePattern {
    raw: String,
}

fn read_gitignore(dir: &Path) -> Result<Vec<IgnorePattern>> {
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
            raw: line.trim_end_matches('/').to_string(),
        })
        .collect())
}

fn ignored(root: &Path, path: &Path, name: &str, patterns: &[IgnorePattern]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    patterns.iter().any(|pattern| {
        pattern.raw == name
            || pattern.raw == relative
            || (pattern.raw.starts_with("*.") && name.ends_with(&pattern.raw[1..]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn skips_hidden_and_gitignored_files() {
        let dir = test_dir("skip");
        fs::write(dir.join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(dir.join("visible.md"), "TODO &visible\n").unwrap();
        fs::write(dir.join("ignored.md"), "TODO &ignored\n").unwrap();
        fs::create_dir(dir.join(".hidden")).unwrap();
        fs::write(dir.join(".hidden").join("x.md"), "TODO &hidden\n").unwrap();

        let files = load_files(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("visible.md"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn honors_grove_extensions_and_max_filesize() {
        let dir = test_dir("settings");
        fs::write(dir.join("keep.todo"), "TODO &keep\n").unwrap();
        fs::write(dir.join("skip.md"), "TODO &skip\n").unwrap();
        fs::write(
            dir.join("large.todo"),
            "TODO &large file with too much content\n",
        )
        .unwrap();

        let files = load_files(&Config::from_groves(vec![GroveConfig {
            root: dir.clone(),
            extensions: vec!["todo".to_string()],
            max_filesize: Some(16),
        }]))
        .unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("keep.todo"));
        fs::remove_dir_all(dir).unwrap();
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
