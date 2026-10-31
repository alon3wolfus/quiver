//! File search for the launcher: an in-memory index of the home folder (`$`),
//! and plocate's prebuilt database for the whole system (`$$`).

use crate::util;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Folders not worth indexing: build output, dependency caches, VCS data.
const SKIP: [&str; 5] = ["node_modules", "target", "__pycache__", "venv", "lost+found"];
pub const MAX_HITS: usize = 50;

pub struct Index {
    paths: Vec<Box<str>>,
    /// Lowercased file names, same order as `paths`, so searching doesn't allocate.
    names: Vec<Box<str>>,
    dirs: Vec<bool>,
    pub built: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: String,
    pub is_dir: bool,
}

impl Index {
    /// Walks `root`, skipping hidden files/folders (like Spotlight) and the SKIP list.
    /// Symlinks aren't followed, so loops can't happen.
    pub fn build(root: &Path) -> Index {
        let mut index = Index { paths: Vec::new(), names: Vec::new(), dirs: Vec::new(), built: Instant::now() };
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with('.') || SKIP.contains(&name) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else { continue };
                let path = entry.path();
                index.paths.push(path.to_string_lossy().into());
                index.names.push(name.to_lowercase().into());
                index.dirs.push(kind.is_dir());
                if kind.is_dir() {
                    stack.push(path);
                }
            }
        }
        index
    }

    pub fn home() -> Index {
        Index::build(&PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Best matches for `query` by file name. Every word must appear in the name.
    pub fn search(&self, query: &str) -> Vec<Hit> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i64, usize)> = self
            .names
            .iter()
            .enumerate()
            .filter_map(|(i, name)| score(name, &words).map(|s| (s - self.paths[i].len() as i64, i)))
            .collect();
        scored.sort_unstable_by_key(|(s, _)| -s);
        scored
            .into_iter()
            .take(MAX_HITS)
            .map(|(_, i)| Hit { path: self.paths[i].to_string(), is_dir: self.dirs[i] })
            .collect()
    }
}

/// Exact name > name starts with the query > a word in it does > it's anywhere in the name.
fn score(name: &str, words: &[String]) -> Option<i64> {
    if !words.iter().all(|w| name.contains(w.as_str())) {
        return None;
    }
    let first = &words[0];
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    Some(if name == first || stem == first {
        4000
    } else if name.starts_with(first.as_str()) {
        3000
    } else if name.split(|c: char| !c.is_alphanumeric()).any(|part| part.starts_with(first.as_str())) {
        2000
    } else {
        1000
    })
}

pub fn plocate_installed() -> bool {
    util::output("which", &["plocate"]).is_ok()
}

/// Whole-system search through plocate's database (instant, updated by its daily timer).
pub fn locate(query: &str) -> Result<Vec<Hit>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let Some(first) = words.first() else { return Ok(Vec::new()) };
    // plocate matches anywhere in the path; ask for plenty and rank by file name.
    let out = util::output("plocate", &["--ignore-case", "--basename", "--limit", "2000", first])?;
    let mut scored: Vec<(i64, &str)> = out
        .lines()
        .filter_map(|path| {
            let name = Path::new(path).file_name()?.to_string_lossy().to_lowercase();
            score(&name, &words).map(|s| (s - path.len() as i64, path))
        })
        .collect();
    scored.sort_unstable_by_key(|(s, _)| -s);
    Ok(scored
        .into_iter()
        .take(MAX_HITS)
        .map(|(_, path)| Hit { path: path.to_string(), is_dir: Path::new(path).is_dir() })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_and_ranks_files() {
        let root = std::env::temp_dir().join(format!("quiver-files-{}", std::process::id()));
        for dir in ["Documents/Reports", "Projects/app/node_modules/pkg", ".secret"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["Documents/report.pdf", "Documents/Reports/old report.txt", "Projects/app/node_modules/pkg/report.js", ".secret/report.key", "notes.md"] {
            std::fs::write(root.join(file), "").unwrap();
        }
        let index = Index::build(&root);
        // Hidden and node_modules entries are skipped.
        assert!(index.paths.iter().all(|p| !p.contains(".secret") && !p.contains("node_modules")));

        let names: Vec<String> = index
            .search("report")
            .iter()
            .map(|h| Path::new(&h.path).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names[0], "report.pdf");
        assert!(names.contains(&"Reports".to_string()));
        assert!(names.contains(&"old report.txt".to_string()));
        assert_eq!(index.search("old report").len(), 1);
        assert!(index.search("Reports")[0].is_dir || index.search("Reports").iter().any(|h| h.is_dir));
        std::fs::remove_dir_all(root).unwrap();
    }
}
