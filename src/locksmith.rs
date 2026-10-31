//! File Locksmith: which processes are using a file or folder, found through /proc.

use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Holder {
    pub pid: u32,
    pub name: String,
    pub command: String,
    /// What it holds, e.g. "open: /home/me/notes.txt" or "working directory: /home/me/proj".
    pub uses: Vec<String>,
}

/// True if `path` is `target` or (when `target` is a folder) inside it.
fn covers(target: &Path, path: &Path) -> bool {
    path == target || path.starts_with(target)
}

fn read_trimmed(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

/// Processes using any of `targets`. Processes of other users are skipped unless
/// quiver runs as root, because their /proc entries can't be read.
pub fn find(targets: &[PathBuf]) -> Vec<Holder> {
    let targets: Vec<PathBuf> = targets.iter().filter_map(|t| t.canonicalize().ok()).collect();
    let me = std::process::id();
    let mut holders = Vec::new();
    let Ok(procs) = std::fs::read_dir("/proc") else { return holders };
    for entry in procs.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if pid == me {
            continue;
        }
        let dir = entry.path();
        let mut uses: Vec<String> = Vec::new();
        let mut note = |kind: &str, path: &Path| {
            if targets.iter().any(|t| covers(t, path)) {
                let line = format!("{kind}: {}", path.display());
                if !uses.contains(&line) {
                    uses.push(line);
                }
            }
        };
        if let Ok(fds) = std::fs::read_dir(dir.join("fd")) {
            for fd in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd.path()) {
                    note("open", &link);
                }
            }
        }
        if let Ok(cwd) = std::fs::read_link(dir.join("cwd")) {
            note("working directory", &cwd);
        }
        if let Ok(exe) = std::fs::read_link(dir.join("exe")) {
            note("running program", &exe);
        }
        // Memory-mapped files (libraries, databases) as listed in /proc/PID/maps.
        for line in std::fs::read_to_string(dir.join("maps")).unwrap_or_default().lines() {
            if let Some(path) = line.split_whitespace().nth(5).filter(|p| p.starts_with('/')) {
                note("loaded", Path::new(path));
            }
        }
        if !uses.is_empty() {
            let command = read_trimmed(dir.join("cmdline")).replace('\0', " ");
            holders.push(Holder { pid, name: read_trimmed(dir.join("comm")), command, uses });
        }
    }
    holders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.pid.cmp(&b.pid)));
    holders
}

/// Asks a process to quit (SIGTERM), like "End task".
pub fn end(pid: u32) -> Result<()> {
    let status = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status()?;
    if !status.success() {
        bail!("couldn't end process {pid} (it may belong to another user)");
    }
    Ok(())
}

pub fn run(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        if !path.exists() {
            bail!("{} doesn't exist", path.display());
        }
    }
    let holders = find(paths);
    if holders.is_empty() {
        println!("Nothing is using {}.", if paths.len() == 1 { "it" } else { "them" });
        return Ok(());
    }
    // Group by program name for a compact list.
    let mut by_name: BTreeMap<&str, Vec<&Holder>> = BTreeMap::new();
    for h in &holders {
        by_name.entry(&h.name).or_default().push(h);
    }
    for (name, list) in by_name {
        for h in list {
            println!("{name} (pid {})", h.pid);
            for u in &h.uses {
                println!("    {u}");
            }
        }
    }
    println!("\nEnd one with: kill <pid>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn finds_a_process_holding_a_file_open() {
        let dir = std::env::temp_dir().join(format!("quiver-locksmith-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("held.txt");
        std::fs::write(&file, "x").unwrap();
        // A child that keeps the file open on stdin and has the folder as its working directory.
        let mut child = Command::new("sleep")
            .arg("5")
            .current_dir(&dir)
            .stdin(Stdio::from(std::fs::File::open(&file).unwrap()))
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let by_file = find(&[file.clone()]);
        let holder = by_file.iter().find(|h| h.pid == child.id()).expect("sleep should hold the file");
        assert!(holder.uses.iter().any(|u| u.starts_with("open:")));

        let by_dir = find(&[dir.clone()]);
        let holder = by_dir.iter().find(|h| h.pid == child.id()).unwrap();
        assert!(holder.uses.iter().any(|u| u.starts_with("working directory:")));

        end(child.id()).unwrap();
        let _ = child.wait();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_covers_its_contents_only() {
        assert!(covers(Path::new("/a/b"), Path::new("/a/b/c.txt")));
        assert!(!covers(Path::new("/a/b"), Path::new("/a/bc.txt")));
    }
}
