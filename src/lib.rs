//! fstrace library: low-level filesystem snapshot + diff primitives.
//!
//! The polling engine behind the `fstrace` CLI, exposed for embedding.
//! Capture with [`snapshot`] (or [`snapshot_hashed`] for fingerprint-level
//! change detection), compare with [`diff`] (or [`diff_ordered`] for stable
//! output), filter with [`matches_exclude`]. You drive the loop and timing.
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
//!     println!("{} {}", event.kind(), event.path());
//! }
//! ```

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// A single filesystem change between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Created { path: String },
    Modified { path: String },
    Deleted { path: String },
}

impl Event {
    /// The affected path, whichever variant carries it. Saves matching
    /// all three variants just to read one field.
    pub fn path(&self) -> &str {
        match self {
            Event::Created { path } | Event::Modified { path } | Event::Deleted { path } => path,
        }
    }

    /// `"created"`, `"modified"` or `"deleted"`.
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Created { .. } => "created",
            Event::Modified { .. } => "modified",
            Event::Deleted { .. } => "deleted",
        }
    }
}

/// Current Unix timestamp in seconds. Shared helper so every consumer
/// stamps events the same way instead of rolling its own clock.
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Last-modified time of `path`, or `None` when unreadable.
/// `None` is still tracked (not skipped) so a file flipping between
/// unreadable and readable registers as modified rather than vanishing.
pub fn mtime(path: &Path) -> Option<SystemTime> {
    stat(path).0
}

/// File size of `path`, or `0` when unreadable.
pub fn fsize(path: &Path) -> u64 {
    stat(path).1
}

// One metadata call serves both mtime and size: two separate stats per
// file per poll would double syscalls on large trees for no reason.
fn stat(path: &Path) -> (Option<SystemTime>, u64) {
    match fs::metadata(path) {
        Ok(m) => (m.modified().ok(), m.len()),
        Err(_) => (None, 0),
    }
}

/// Files at or below this size get content-hashed by [`snapshot_hashed`].
/// Larger files skip hashing: re-reading gigabytes every poll would cost
/// more than the misses it prevents.
pub const MAX_HASH_BYTES: u64 = 4 * 1024 * 1024;

/// Fingerprint of `path`'s content, or `None` when unreadable or larger
/// than [`MAX_HASH_BYTES`]. Deterministic FNV-1a (hand-rolled to keep the
/// zero-dependency core): identical content hashes identically across
/// polls and processes, which change detection requires. A randomized
/// hasher would report every file modified on every poll.
pub fn fhash(path: &Path) -> Option<u64> {
    use std::io::Read;

    let mut file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_HASH_BYTES {
        return None;
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut buf = [0u8; 8192];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for byte in &buf[..n] {
                    hash ^= *byte as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
            Err(_) => return None,
        }
    }
    Some(hash)
}

/// Tracked state of one file. Compared wholesale: any field differing
/// between snapshots counts as modified. `modified` alone misses
/// same-stamp rewrites (issue #3); `size` catches most of those, and
/// `hash`, filled only by [`snapshot_hashed`], catches the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub modified: Option<SystemTime>,
    pub size: u64,
    pub hash: Option<u64>,
}

/// Map of `path -> state` for every file under `dir`, skipping `excludes`
/// (see [`matches_exclude`] for the matching rules). Tracks mtime plus
/// size, so same-stamp rewrites of different sizes are caught. Deliberately
/// no content hashing: the default stays as cheap as the v1 CLI (one
/// metadata call per file). Reach for [`snapshot_hashed`] when same-size
/// rewrites must be caught too.
pub fn snapshot(dir: &Path, excludes: &[String]) -> HashMap<String, FileState> {
    snapshot_inner(dir, excludes, false)
}

/// Like [`snapshot`], but also fingerprints file contents (up to
/// [`MAX_HASH_BYTES`]). Costs a read per file per poll, so use it for
/// source trees, not for gigabytes of assets.
pub fn snapshot_hashed(dir: &Path, excludes: &[String]) -> HashMap<String, FileState> {
    snapshot_inner(dir, excludes, true)
}

// Single collection path for both modes: two copies would drift apart the
// first time someone fixes a bug in one and forgets the other.
fn snapshot_inner(dir: &Path, excludes: &[String], hash: bool) -> HashMap<String, FileState> {
    let mut map = HashMap::new();
    let mut visited = HashSet::new();
    collect(dir, &mut map, excludes, hash, &mut visited);
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
///   only imprecision on non-ASCII names, fine for an exclude filter);
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
// Walk `dir` into `map`. Unreadable directories are skipped silently:
// one permission-denied folder must not kill the whole watch.
// `visited` holds canonicalized directories already entered: a shortcut
// pointing back above itself resolves to a visited path, so the walk
// stops instead of circling forever. Canonicalize (not dev/ino) keeps
// this portable across the platforms the CLI ships to.
fn collect(
    dir: &Path,
    map: &mut HashMap<String, FileState>,
    excludes: &[String],
    hash: bool,
    visited: &mut HashSet<PathBuf>,
) {
    let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canonical) {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let path_str = path.to_string_lossy();

            if excludes.iter().any(|e| matches_exclude(&path_str, name, e)) {
                continue;
            }

            if path.is_file() {
                let state = FileState {
                    modified: mtime(&path),
                    size: fsize(&path),
                    hash: if hash { fhash(&path) } else { None },
                };
                map.insert(path_str.into_owned(), state);
            } else if path.is_dir() {
                collect(&path, map, excludes, hash, visited);
            }
        }
    }
}

/// Pure diff: every change from `prev` to `curr`, in the same order the
/// CLI has always emitted (current entries first, deletions after).
/// Two passes because the questions differ: created/modified ask "what's
/// in curr that's new or changed", deleted asks "what's in prev that's
/// gone from curr".
pub fn diff(prev: &HashMap<String, FileState>, curr: &HashMap<String, FileState>) -> Vec<Event> {
    let mut events = Vec::new();

    for (key, state) in curr {
        match prev.get(key) {
            None => events.push(Event::Created { path: key.clone() }),
            Some(old) if old != state => events.push(Event::Modified { path: key.clone() }),
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

/// Like [`diff`], but sorted by path. Opt-in: the default stays in
/// emission order so existing consumers see zero behavior change. Reach
/// for this when batching output or asserting on it in tests.
pub fn diff_ordered(
    prev: &HashMap<String, FileState>,
    curr: &HashMap<String, FileState>,
) -> Vec<Event> {
    let mut events = diff(prev, curr);
    events.sort_by(|a, b| a.path().cmp(b.path()));
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn snap(pairs: &[(&str, FileState)]) -> HashMap<String, FileState> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn t(secs: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(secs))
    }

    // Plain snapshot state: mtime + size, no hash. This is what `snapshot` records.
    fn st(secs: u64, size: u64) -> FileState {
        FileState {
            modified: t(secs),
            size,
            hash: None,
        }
    }

    #[test]
    fn detects_created_modified_deleted() {
        let prev = snap(&[("a", st(1, 10)), ("b", st(1, 10)), ("c", st(1, 10))]);
        let curr = snap(&[("a", st(1, 10)), ("b", st(2, 10)), ("d", st(1, 10))]);
        let mut events = diff(&prev, &curr);
        events.sort_by(|a, b| (a.kind(), a.path()).cmp(&(b.kind(), b.path())));
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
    fn ordered_mode_is_stable_and_matches_default_set() {
        // Same input twice: identical order both times, and the same set
        // the default mode reports (order aside).
        let prev = snap(&[("b", st(1, 10)), ("a", st(1, 10))]);
        let curr = snap(&[("b", st(2, 10)), ("c", st(1, 10))]);
        let first = diff_ordered(&prev, &curr);
        let second = diff_ordered(&prev, &curr);
        assert_eq!(first, second);
        let paths: Vec<&str> = first.iter().map(|e| e.path()).collect();
        assert_eq!(paths, vec!["a", "b", "c"]);
        let mut def: Vec<String> = diff(&prev, &curr)
            .iter()
            .map(|e| format!("{}:{}", e.kind(), e.path()))
            .collect();
        def.sort();
        let mut ord: Vec<String> = first
            .iter()
            .map(|e| format!("{}:{}", e.kind(), e.path()))
            .collect();
        ord.sort();
        assert_eq!(def, ord);
    }

    #[test]
    fn path_reads_off_all_variants() {
        assert_eq!(Event::Created { path: "c".into() }.path(), "c");
        assert_eq!(Event::Modified { path: "m".into() }.path(), "m");
        assert_eq!(Event::Deleted { path: "d".into() }.path(), "d");
    }

    #[test]
    fn identical_snapshots_emit_nothing() {
        let prev = snap(&[("a", st(1, 10))]);
        let curr = snap(&[("a", st(1, 10))]);
        assert!(diff(&prev, &curr).is_empty());
    }

    #[test]
    fn same_stamp_different_size_counts_as_modified() {
        // The live-caught case class: timestamp unchanged, content changed.
        let prev = snap(&[("a", st(1, 10))]);
        let curr = snap(&[("a", st(1, 20))]);
        assert_eq!(
            diff(&prev, &curr),
            vec![Event::Modified { path: "a".into() }]
        );
    }

    #[test]
    fn same_stamp_same_size_different_hash_counts_as_modified() {
        // Strongest case from the field: v1 -> v2 rewrites that keep both
        // stamp and size. Only the fingerprint sees them, which is why the
        // hash exists at all.
        let prev = snap(&[(
            "a",
            FileState {
                modified: t(1),
                size: 2,
                hash: Some(111),
            },
        )]);
        let curr = snap(&[(
            "a",
            FileState {
                modified: t(1),
                size: 2,
                hash: Some(222),
            },
        )]);
        assert_eq!(
            diff(&prev, &curr),
            vec![Event::Modified { path: "a".into() }]
        );
    }

    #[test]
    fn hash_is_deterministic_across_calls() {
        // Regression: a randomized hasher reports every file modified on
        // every poll. Same content must hash identically, always.
        let dir = std::env::temp_dir().join(format!("fstrace-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        std::fs::write(&a, "same content").unwrap();
        std::fs::write(&b, "same content").unwrap();
        std::fs::write(dir.join("c.txt"), "different").unwrap();
        assert_eq!(fhash(&a), fhash(&b));
        assert_eq!(fhash(&a), fhash(&a));
        assert_ne!(fhash(&a), fhash(&dir.join("c.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn circular_shortcut_finishes_scan() {
        // A shortcut pointing back above itself must not circle forever.
        // Unix-only: creating symlinks needs privileges on Windows.
        let dir = std::env::temp_dir().join(format!("fstrace-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let inner = dir.join("a").join("b");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("real.txt"), "data").unwrap();
        std::os::unix::fs::symlink(dir.join("a"), inner.join("loop")).unwrap();
        let map = snapshot(&dir, &[]);
        assert!(map.keys().any(|k| k.ends_with("real.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_mtime_change_counts_as_modified() {
        let prev = snap(&[(
            "a",
            FileState {
                modified: None,
                size: 10,
                hash: None,
            },
        )]);
        let curr = snap(&[("a", st(5, 10))]);
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
