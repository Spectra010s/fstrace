use std::{
    collections::HashMap,
    env,
    fs,
    path::Path,
    process,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ANSI colors
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const DIM: &str = "\x1b[2m";

fn print_help() {
    println!("
{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}
A lightweight file system watcher for files and folders.

{BOLD}Usage:{RESET}
  fstrace <path> [options]   Watch a file or folder
  fstrace -v, --version      Show version
  fstrace -h, --help         Show this help message

{BOLD}Options:{RESET}
  --json    Output events as JSON (for programmatic use)

{BOLD}Events:{RESET}
  {GREEN}[created]{RESET}   A file was created
  {YELLOW}[modified]{RESET}  A file was modified
  {RED}[deleted]{RESET}   A file was deleted
");
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn emit(event: &str, path: &str, json: bool) {
    if json {
        println!(
            "{{\"event\":\"{event}\",\"path\":\"{path}\",\"timestamp\":{}}}",
            timestamp()
        );
    } else {
        let time = now();
        match event {
            "created" => println!("{DIM}[{time}]{RESET} {GREEN}[created]{RESET}   {path}"),
            "modified" => println!("{DIM}[{time}]{RESET} {YELLOW}[modified]{RESET}  {path}"),
            "deleted" => println!("{DIM}[{time}]{RESET} {RED}[deleted]{RESET}   {path}"),
            _ => {}
        }
    }
}

fn get_modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = duration % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn snapshot(dir: &Path) -> HashMap<String, Option<SystemTime>> {
    let mut map = HashMap::new();
    collect(dir, &mut map);
    map
}

fn collect(dir: &Path, map: &mut HashMap<String, Option<SystemTime>>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let key = path.to_string_lossy().to_string();
                let modified = get_modified(&path);
                map.insert(key, modified);
            } else if path.is_dir() {
                collect(&path, map);
            }
        }
    }
}

fn watch_file(path: &Path, json: bool) {
    if !json {
        println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
        println!("{DIM}Watching file: {}{RESET}\n", path.display());
    }

    let mut last = get_modified(path);

    loop {
        thread::sleep(Duration::from_millis(500));

        let path_str = path.to_string_lossy();

        if !path.exists() {
            emit("deleted", &path_str, json);
            break;
        }

        let current = get_modified(path);
        if current != last {
            emit("modified", &path_str, json);
            last = current;
        }
    }
}

fn watch_folder(path: &Path, json: bool) {
    if !json {
        println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
        println!("{DIM}Watching folder: {}{RESET}\n", path.display());
    }

    let mut prev = snapshot(path);

    loop {
        thread::sleep(Duration::from_millis(500));

        let current = snapshot(path);

        for (key, modified) in &current {
            match prev.get(key) {
                None => emit("created", key, json),
                Some(old) if old != modified => emit("modified", key, json),
                _ => {}
            }
        }

        for key in prev.keys() {
            if !current.contains_key(key) {
                emit("deleted", key, json);
            }
        }

        prev = current;
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("-h") | Some("--help") | Some("help") => {
            print_help();
        }
        Some("-v") | Some("--version") => {
            println!("fstrace v{VERSION}");
        }
        Some(path) => {
            let json = args.contains(&"--json".to_string());
            let p = Path::new(path);
            if !p.exists() {
                eprintln!("{RED}Error:{RESET} path not found: {path}");
                process::exit(1);
            }
            if p.is_dir() {
                watch_folder(p, json);
            } else {
                watch_file(p, json);
            }
        }
        None => {
            eprintln!("{RED}Error:{RESET} no path provided\n");
            print_help();
            process::exit(1);
        }
    }
}

