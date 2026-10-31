use anyhow::{Context, Result, bail};
use regex::{NoExpand, Regex, RegexBuilder};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Options {
    pub find: String,
    pub replace: String,
    pub files: Vec<PathBuf>,
    pub regex: bool,
    pub ignore_case: bool,
    pub yes: bool,
}

/// Works out each file's new name. `{n}` in the replacement becomes a counter
/// (1, 2, 3...). Files whose name doesn't change are left out.
pub fn plan(opts: &Options) -> Result<Vec<(PathBuf, PathBuf)>> {
    let pattern = if opts.regex { opts.find.clone() } else { regex::escape(&opts.find) };
    let re: Regex = RegexBuilder::new(&pattern)
        .case_insensitive(opts.ignore_case)
        .build()
        .context("invalid search pattern")?;

    let mut renames = Vec::new();
    let mut counter = 0;
    for path in &opts.files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("not a valid file name: {}", path.display()))?;
        if !re.is_match(name) {
            continue;
        }
        counter += 1;
        let replace = opts.replace.replace("{n}", &counter.to_string());
        // In regex mode $1, $2... insert capture groups; otherwise $ is literal.
        let new_name = if opts.regex {
            re.replace_all(name, replace.as_str()).into_owned()
        } else {
            re.replace_all(name, NoExpand(&replace)).into_owned()
        };
        if new_name != name {
            renames.push((path.clone(), path.with_file_name(new_name)));
        }
    }
    Ok(renames)
}

/// Refuses renames that would overwrite something or give two files one name.
pub fn check(renames: &[(PathBuf, PathBuf)]) -> Result<()> {
    let sources: HashSet<&Path> = renames.iter().map(|(from, _)| from.as_path()).collect();
    let mut targets = HashSet::new();
    for (from, to) in renames {
        let name = to.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.is_empty() || name.contains('/') {
            bail!("{} would get an empty or invalid name", from.display());
        }
        if !targets.insert(to.as_path()) {
            bail!("two files would both be named {}", to.display());
        }
        // A target that's also being renamed away is fine; anything else would be overwritten.
        if to.exists() && !sources.contains(to.as_path()) {
            bail!("{} already exists", to.display());
        }
    }
    Ok(())
}

pub fn run(opts: Options) -> Result<()> {
    let renames = plan(&opts)?;
    if renames.is_empty() {
        println!("Nothing to rename.");
        return Ok(());
    }
    check(&renames)?;
    for (from, to) in &renames {
        println!("{}  →  {}", from.display(), to.file_name().unwrap().to_string_lossy());
    }
    if !opts.yes {
        println!("\nPreview only. Add --yes to rename {} file(s).", renames.len());
        return Ok(());
    }
    apply(&renames)?;
    println!("\nRenamed {} file(s).", renames.len());
    Ok(())
}

/// Renames files on disk. Call `check` first.
pub fn apply(renames: &[(PathBuf, PathBuf)]) -> Result<()> {
    // Two steps (old → temp → new) so swaps like a→b, b→a don't clobber each other.
    let temps: Vec<PathBuf> = renames
        .iter()
        .enumerate()
        .map(|(i, (from, _))| from.with_file_name(format!(".quiver-rename-{}-{i}", std::process::id())))
        .collect();
    for ((from, _), temp) in renames.iter().zip(&temps) {
        std::fs::rename(from, temp).with_context(|| format!("renaming {}", from.display()))?;
    }
    for ((_, to), temp) in renames.iter().zip(&temps) {
        std::fs::rename(temp, to).with_context(|| format!("renaming to {}", to.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(find: &str, replace: &str, files: &[&str], regex: bool) -> Options {
        Options {
            find: find.into(),
            replace: replace.into(),
            files: files.iter().map(PathBuf::from).collect(),
            regex,
            ignore_case: false,
            yes: false,
        }
    }

    fn names(renames: &[(PathBuf, PathBuf)]) -> Vec<String> {
        renames.iter().map(|(_, to)| to.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn literal_replace_keeps_dollar_signs() {
        let r = plan(&opts("draft", "$final", &["dir/draft.txt", "notes.txt"], false)).unwrap();
        assert_eq!(names(&r), ["dir/$final.txt"]);
    }

    #[test]
    fn regex_with_captures_and_counter() {
        let o = opts(r"^IMG_(\d+)", "holiday-{n}-$1", &["IMG_0042.jpg", "IMG_0043.jpg"], true);
        assert_eq!(names(&plan(&o).unwrap()), ["holiday-1-0042.jpg", "holiday-2-0043.jpg"]);
    }

    #[test]
    fn rejects_two_files_with_one_name() {
        let r = plan(&opts(r"\d", "", &["a1.txt", "a2.txt"], true)).unwrap();
        assert!(check(&r).is_err());
    }

    #[test]
    fn swaps_names_without_losing_files() {
        let dir = std::env::temp_dir().join(format!("quiver-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.txt"), "first").unwrap();
        std::fs::write(dir.join("2.txt"), "second").unwrap();
        // Counter order follows the file list, so 2.txt → 1.txt and 1.txt → 2.txt.
        let mut o = opts(r"^\d", "{n}", &[], true);
        o.files = vec![dir.join("2.txt"), dir.join("1.txt")];
        o.yes = true;
        run(o).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("1.txt")).unwrap(), "second");
        assert_eq!(std::fs::read_to_string(dir.join("2.txt")).unwrap(), "first");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
