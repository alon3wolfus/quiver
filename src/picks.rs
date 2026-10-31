//! Remembers which launcher result you choose for each search, so a result you keep
//! picking from further down moves to the top for that search.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Picks needed before a result is promoted.
pub const THRESHOLD: u32 = 3;
/// Searches remembered; the least used are forgotten past this.
const MAX_QUERIES: usize = 500;

#[derive(Default, Serialize, Deserialize)]
pub struct Picks {
    /// Search text (trimmed, lowercased) → result key → times picked.
    counts: HashMap<String, HashMap<String, u32>>,
}

fn normalise(query: &str) -> String {
    query.trim().to_lowercase()
}

fn path() -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("quiver/launcher-picks.json")
}

impl Picks {
    pub fn load() -> Picks {
        Picks::load_from(&path())
    }

    fn load_from(path: &Path) -> Picks {
        std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    fn save_to(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }

    /// Counts a pick of `key` for `query` and saves.
    pub fn record(&mut self, query: &str, key: &str) {
        self.record_in(query, key, &path());
    }

    fn record_in(&mut self, query: &str, key: &str, path: &Path) {
        let query = normalise(query);
        if query.is_empty() {
            return;
        }
        *self.counts.entry(query).or_default().entry(key.to_string()).or_default() += 1;
        if self.counts.len() > MAX_QUERIES {
            let total = |picks: &HashMap<String, u32>| picks.values().sum::<u32>();
            if let Some(least) = self.counts.iter().min_by_key(|(_, p)| total(p)).map(|(q, _)| q.clone()) {
                self.counts.remove(&least);
            }
        }
        self.save_to(path);
    }

    /// Moves results picked at least THRESHOLD times for `query` to the top, most
    /// picked first. Everything else keeps its order.
    pub fn rank<T>(&self, query: &str, items: &mut Vec<T>, key: impl Fn(&T) -> Option<String>) {
        let Some(picks) = self.counts.get(&normalise(query)) else { return };
        let count = |item: &T| key(item).and_then(|k| picks.get(&k).copied()).filter(|&c| c >= THRESHOLD).unwrap_or(0);
        // A stable sort, so ties and unpicked results stay where search put them.
        items.sort_by_key(|item| std::cmp::Reverse(count(item)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_a_result_after_three_picks() {
        let path = std::env::temp_dir().join(format!("quiver-picks-{}.json", std::process::id()));
        let mut picks = Picks::default();
        let key = |s: &&str| Some(s.to_string());
        let ranked = |picks: &Picks, query| {
            let mut items = vec!["firefox", "filezilla", "file roller"];
            picks.rank(query, &mut items, key);
            items
        };

        picks.record_in("fi", "filezilla", &path);
        picks.record_in("Fi ", "filezilla", &path);
        assert_eq!(ranked(&picks, "fi"), ["firefox", "filezilla", "file roller"]);
        picks.record_in("fi", "filezilla", &path);
        assert_eq!(ranked(&picks, "fi"), ["filezilla", "firefox", "file roller"]);
        // Only for that search.
        assert_eq!(ranked(&picks, "fil"), ["firefox", "filezilla", "file roller"]);
        // And it's saved.
        assert_eq!(ranked(&Picks::load_from(&path), "fi")[0], "filezilla");
        std::fs::remove_file(path).unwrap();
    }
}
