//! fstrace library: low-level filesystem snapshot + diff primitives.
//!
//! The polling engine behind the `fstrace` CLI, exposed for embedding:
//! snapshot a tree, diff two snapshots, drive your own loop and timing.
//!
//! Polling (not OS event hooks) is deliberate: zero dependencies and
//! identical behavior on every platform the CLI ships to.
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
/// `None` is still tracked (not skipped) so a file flipping between
/// unreadable and readable registers as modified rather than vanishing.
pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Map of `path -> mtime` for every file under `dir`, skipping `excludes`
/// (see [`matches_exclude`] for the matching rules).
pub fn snapshot(dir: &Path, excludes: &[String]) -> HashMap<String, Option<SystemTime>> {
    let mut map = HashMap::new();
    collect(dir, &mut map, excludes);
    map
}

/// Whether `pattern` excludes the given path. Plain patterns keep the
/// original rule (exact bare-name match); patterns containing `*` or `?`
/// match as globs against the full path first, then the bare name.
/// A `/`-pattern also matches rooted anywhere below the watch root because
/// users mean "every `dist` tree", never "only a top-level one".
pub fn matches_exclude(path: &str, name: &str, pattern: &str) -> bool {
    if !pattern.contains(['*', '?']) {
        return name == pattern;
    }
    if glob_match(pattern, path) || glob_match(pattern, name) {
        return true;
    }
    // A pattern with a `/` also matches rooted anywhere below the watch
    // root: `dist/**` excludes every `dist` tree, not just a top-level one.
    if pattern.contains('/') {
        let anywhere = format!("*/{pattern}");
        return glob_match(&anywhere, path);
    }
    false
}

/// Minimal glob: `*` spans any characters (including `/`), `?` spans one.
/// Hand-rolled backtracking instead of a regex/glob crate to keep the
/// zero-dependency core. Two deliberate limits, documented so nobody
/// "fixes" them by accident:
/// - byte-wise: `?` spans one byte, not one Unicode char (no panic risk,
///   only imprecision on non-ASCII names — fine for an exclude filter);
/// - `*` spans `/`, so `*.log` also matches `deep/app.log` and `dist/**`
///   matches every `dist` tree (matched anywhere-below-root by the caller).
fn glob_match(pattern: &str, text: &str) -> bool {
    let (px, tx) = (pattern.as_bytes(), text.as_bytes());
    let (mut star, mut mark) = (None, 0usize);
    let (mut pi, mut ti) = (0usize, 0usize);
    while ti < tx.len() {
        if pi < px.len() && (px[pi] == b'?' || px[pi] == tx[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < px.len() && px[pi] == b'*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < px.len() && px[pi] == b'*' {
        pi += 1;
    }
    pi == px.len()
}

// Walk `dir` into `map`. Unreadable directories are skipped silently:
// one permission-denied folder must not kill the whole watch.
fn collect(dir: &Path, map: &mut HashMap<String, Option<SystemTime>>, excludes: &[String]) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let path_str = path.to_string_lossy();

            if excludes.iter().any(|e| matches_exclude(&path_str, name, e)) {
                continue;
            }

            if path.is_file() {
                let modified = mtime(&path);
                map.insert(path_str.into_owned(), modified);
            } else if path.is_dir() {
                collect(&path, map, excludes);
            }
        }
    }
}

/// Pure diff: every change from `prev` to `curr`, in the same order the
/// CLI has always emitted (current entries first, deletions after).
/// Two passes because the questions differ: created/modified ask "what's
/// in curr that's new or changed", deleted asks "what's in prev that's
/// gone from curr".
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

    #[test]
    fn exclude_matching() {
        // exact names behave as before
        assert!(matches_exclude(
            "/w/node_modules/a.js",
            "node_modules",
            "node_modules"
        ));
        assert!(!matches_exclude("/w/src/a.js", "a.js", "node_modules"));
        // star patterns match names and paths
        assert!(matches_exclude("/w/app.log", "app.log", "*.log"));
        assert!(matches_exclude("/w/deep/app.log", "app.log", "*.log"));
        assert!(!matches_exclude("/w/app.txt", "app.txt", "*.log"));
        assert!(matches_exclude("/w/dist/out.js", "out.js", "dist/**"));
        assert!(!matches_exclude("/w/src/out.js", "out.js", "dist/**"));
        // question mark spans exactly one character
        assert!(matches_exclude("/w/a1", "a1", "a?"));
        assert!(!matches_exclude("/w/a12", "a12", "a?"));
    }
}
