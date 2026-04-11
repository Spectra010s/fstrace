use std::{
    collections::HashMap,
    env,
    fs,
    path::Path,
    process,
    thread,
    time::{Duration, SystemTime},
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
  fstrace <path>          Watch a file or folder
  fstrace -v, --version   Show version
  fstrace -h, --help      Show this help message

{BOLD}Events:{RESET}
  {GREEN}[created]{RESET}   A file was created
  {YELLOW}[modified]{RESET}  A file was modified
  {RED}[deleted]{RESET}   A file was deleted
");
}

fn get_modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn snapshot(dir: &Path) -> HashMap<String, Option<SystemTime>> {
    let mut map = HashMap::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let key = path.to_string_lossy().to_string();
                let modified = get_modified(&path);
                map.insert(key, modified);
            }
        }
    }

    map
}

fn watch_file(path: &Path) {
    println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
    println!("{DIM}Watching file: {}{RESET}\n", path.display());

    let mut last = get_modified(path);

    loop {
        thread::sleep(Duration::from_millis(500));

        if !path.exists() {
            println!("{RED}[deleted]{RESET}  {}", path.display());
            break;
        }

        let current = get_modified(path);
        if current != last {
            println!("{YELLOW}[modified]{RESET} {}", path.display());
            last = current;
        }
    }
}

fn watch_folder(path: &Path) {
    println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
    println!("{DIM}Watching folder: {}{RESET}\n", path.display());

    let mut prev = snapshot(path);

    loop {
        thread::sleep(Duration::from_millis(500));

        let current = snapshot(path);

        // Check for created or modified
        for (key, modified) in &current {
            match prev.get(key) {
                None => {
                    println!("{GREEN}[created]{RESET}   {key}");
                }
                Some(old) if old != modified => {
                    println!("{YELLOW}[modified]{RESET}  {key}");
                }
                _ => {}
            }
        }

        // Check for deleted
        for key in prev.keys() {
            if !current.contains_key(key) {
                println!("{RED}[deleted]{RESET}   {key}");
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
            let p = Path::new(path);
            if !p.exists() {
                eprintln!("{RED}Error:{RESET} path not found: {path}");
                process::exit(1);
            }
            if p.is_dir() {
                watch_folder(p);
            } else {
                watch_file(p);
            }
        }
        None => {
            eprintln!("{RED}Error:{RESET} no path provided\n");
            print_help();
            process::exit(1);
        }
    }
}

