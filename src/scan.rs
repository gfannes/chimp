use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{Config, FileId, GroveConfig, Result, SourceFile};

pub fn load_files(config: &Config) -> Result<Vec<SourceFile>> {
    load_files_with_reporter(config, 0, |_| {})
}

pub fn load_files_with_reporter(
    config: &Config,
    verbose: u8,
    report: impl FnMut(&Path),
) -> Result<Vec<SourceFile>> {
    load_files_with_reporter_impl(config, verbose, report, None)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanMeasurements {
    pub directory_traversal: Duration,
    pub gitignore: Duration,
    pub entry_enumeration: Duration,
    pub entry_sorting: Duration,
    pub metadata_checks: Duration,
    pub file_reads: Duration,
    pub utf8_conversion: Duration,
    pub other: Duration,
    pub directories_visited: usize,
    pub candidate_files: usize,
    pub loaded_files: usize,
    pub bytes_read: usize,
}

pub fn load_files_with_reporter_measured(
    config: &Config,
    verbose: u8,
    report: impl FnMut(&Path),
) -> Result<(Vec<SourceFile>, ScanMeasurements)> {
    let mut measurements = ScanMeasurements::default();
    let files = load_files_with_reporter_impl(config, verbose, report, Some(&mut measurements))?;
    Ok((files, measurements))
}

fn load_files_with_reporter_impl(
    config: &Config,
    verbose: u8,
    mut report: impl FnMut(&Path),
    mut measurements: Option<&mut ScanMeasurements>,
) -> Result<Vec<SourceFile>> {
    let traversal_started = measurements.as_ref().map(|_| Instant::now());
    let mut files = Vec::new();
    for (grove, grove_config) in config.groves.iter().enumerate() {
        let root = grove_config.root.canonicalize().map_err(|error| {
            format!(
                "failed to access Grove {}: {error}",
                grove_config.root.display()
            )
        })?;
        walk_root(
            grove,
            grove_config,
            &root,
            &root,
            Vec::new(),
            &mut files,
            verbose,
            &mut report,
            measurements.as_deref_mut(),
        )?;
    }
    if let (Some(measurements), Some(started)) = (measurements, traversal_started) {
        measurements.directory_traversal = started.elapsed();
        let accounted = measurements.gitignore
            + measurements.entry_enumeration
            + measurements.entry_sorting
            + measurements.metadata_checks
            + measurements.file_reads
            + measurements.utf8_conversion;
        measurements.other = measurements.directory_traversal.saturating_sub(accounted);
    }
    Ok(files)
}

#[allow(clippy::too_many_arguments)]
fn walk_root(
    grove: usize,
    grove_config: &GroveConfig,
    root: &Path,
    dir: &Path,
    inherited_ignore: Vec<IgnorePattern>,
    files: &mut Vec<SourceFile>,
    verbose: u8,
    report: &mut impl FnMut(&Path),
    mut measurements: Option<&mut ScanMeasurements>,
) -> Result<()> {
    if let Some(measurements) = measurements.as_deref_mut() {
        measurements.directories_visited += 1;
    }
    if verbose >= 3 {
        report(dir);
    }
    let mut ignore = inherited_ignore;
    let started = measurements.as_ref().map(|_| Instant::now());
    ignore.extend(read_gitignore(root, dir)?);
    if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started) {
        measurements.gitignore += started.elapsed();
    }

    let started = measurements.as_ref().map(|_| Instant::now());
    let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started) {
        measurements.entry_enumeration += started.elapsed();
    }
    let started = measurements.as_ref().map(|_| Instant::now());
    // All entries share this directory's parent, so filename order is
    // equivalent to full-path order. Cache the key once per entry.
    entries.sort_by_cached_key(|entry| entry.file_name());
    if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started) {
        measurements.entry_sorting += started.elapsed();
    }

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
            walk_root(
                grove,
                grove_config,
                root,
                &path,
                ignore.clone(),
                files,
                verbose,
                report,
                measurements.as_deref_mut(),
            )?;
        } else if file_type.is_file() && is_supported_file(&path, grove_config) {
            if let Some(measurements) = measurements.as_deref_mut() {
                measurements.candidate_files += 1;
            }
            if grove_config.max_filesize.is_some_and(|max| {
                let started = measurements.as_ref().map(|_| Instant::now());
                let too_large = entry.metadata().is_ok_and(|metadata| metadata.len() > max);
                if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started)
                {
                    measurements.metadata_checks += started.elapsed();
                }
                too_large
            }) {
                continue;
            }
            if verbose >= 3 {
                report(&path);
            }
            let started = measurements.as_ref().map(|_| Instant::now());
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started) {
                measurements.file_reads += started.elapsed();
                measurements.bytes_read += bytes.len();
                measurements.loaded_files += 1;
            }
            let started = measurements.as_ref().map(|_| Instant::now());
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if let (Some(measurements), Some(started)) = (measurements.as_deref_mut(), started) {
                measurements.utf8_conversion += started.elapsed();
            }
            let id = FileId(files.len());
            files.push(SourceFile {
                id,
                grove,
                root: root.to_path_buf(),
                path,
                bytes,
                text: Arc::new(text),
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

    #[test]
    fn honors_anchored_directory_gitignore_patterns() {
        let dir = test_dir("anchored-ignore");
        fs::write(dir.join(".gitignore"), "/target/\n").unwrap();
        fs::create_dir(dir.join("target")).unwrap();
        fs::write(dir.join("target").join("ignored.md"), "TODO &ignored\n").unwrap();
        fs::create_dir(dir.join("nested")).unwrap();
        fs::create_dir(dir.join("nested").join("target")).unwrap();
        fs::write(
            dir.join("nested").join("target").join("kept.md"),
            "TODO &kept\n",
        )
        .unwrap();

        let files = load_files(&Config::from_roots(vec![dir.clone()])).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("nested/target/kept.md"));
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
