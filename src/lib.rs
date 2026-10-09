//! fstrace library: low-level filesystem snapshot + diff primitives.
//!
//! The polling engine behind the `fstrace` CLI, exposed for embedding:
//! snapshot a tree, diff two snapshots, drive your own loop and timing.
//!
//! ```no_run
//! use std::path::Path;
//! use fstrace::{snapshot, diff};
//!
//! let excludes: Vec<String> = vec!["node_modules".into()];
//! let prev = snapshot(Path::new("./project"), &excludes);
//! // ... later ...
//! let curr = snapshot(Path::new("./project"), &excludes);
//! for event in diff(&prev, &curr) {
//!     println!("{}", event.kind());
//! }
//! ```

use std::{collections::HashMap, fs, path::Path, time::SystemTime};

/// A single filesystem change between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Created { path: String },
    Modified { path: String },
    Deleted { path: String },
}

impl Event {
    /// `"created"`, `"modified"` or `"deleted"`.
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Created { .. } => "created",
            Event::Modified { .. } => "modified",
            Event::Deleted { .. } => "deleted",
        }
    }
}

/// Last-modified time of `path`, or `None` when unreadable.
pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Map of `path -> mtime` for every file under `dir`, skipping `excludes`
/// (matched against bare file/folder names, same rule as the CLI).
pub fn snapshot(dir: &Path, excludes: &[String]) -> HashMap<String, Option<SystemTime>> {
    let mut map = HashMap::new();
    collect(dir, &mut map, excludes);
    map
}

fn collect(dir: &Path, map: &mut HashMap<String, Option<SystemTime>>, excludes: &[String]) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if excludes.iter().any(|e| e == name) {
                continue;
            }

            if path.is_file() {
                let key = path.to_string_lossy().to_string();
                map.insert(key, mtime(&path));
            } else if path.is_dir() {
                collect(&path, map, excludes);
            }
        }
    }
}

/// Pure diff: every change from `prev` to `curr`, in the same order the
/// CLI has always emitted (current entries first, deletions after).
pub fn diff(
    prev: &HashMap<String, Option<SystemTime>>,
    curr: &HashMap<String, Option<SystemTime>>,
) -> Vec<Event> {
    let mut events = Vec::new();

    for (key, modified) in curr {
        match prev.get(key) {
            None => events.push(Event::Created { path: key.clone() }),
            Some(old) if old != modified => events.push(Event::Modified { path: key.clone() }),
            _ => {}
        }
    }

    for key in prev.keys() {
        if !curr.contains_key(key) {
            events.push(Event::Deleted { path: key.clone() });
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn snap(pairs: &[(&str, Option<SystemTime>)]) -> HashMap<String, Option<SystemTime>> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn t(secs: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(secs))
    }

    fn event_path(e: &Event) -> &str {
        match e {
            Event::Created { path } | Event::Modified { path } | Event::Deleted { path } => path,
        }
    }

    #[test]
    fn detects_created_modified_deleted() {
        let prev = snap(&[("a", t(1)), ("b", t(1)), ("c", t(1))]);
        let curr = snap(&[("a", t(1)), ("b", t(2)), ("d", t(1))]);
        let mut events = diff(&prev, &curr);
        events.sort_by(|a, b| (a.kind(), event_path(a)).cmp(&(b.kind(), event_path(b))));
        assert_eq!(
            events,
            vec![
                Event::Created { path: "d".into() },
                Event::Deleted { path: "c".into() },
                Event::Modified { path: "b".into() },
            ]
        );
    }

    #[test]
    fn identical_snapshots_emit_nothing() {
        let prev = snap(&[("a", t(1))]);
        let curr = snap(&[("a", t(1))]);
        assert!(diff(&prev, &curr).is_empty());
    }

    #[test]
    fn unreadable_mtime_change_counts_as_modified() {
        let prev = snap(&[("a", None)]);
        let curr = snap(&[("a", t(5))]);
        assert_eq!(
            diff(&prev, &curr),
            vec![Event::Modified { path: "a".into() }]
        );
    }
}
