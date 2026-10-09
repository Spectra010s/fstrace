use std::{
    collections::HashMap,
    env,
    path::Path,
    process, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use fstrace::{Event, diff, mtime, snapshot};

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
    println!(
        "
{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}
A lightweight file system watcher for files and folders.

{BOLD}Usage:{RESET}
  fstrace <path> [options]   Watch a file or folder
  fstrace -v, --version      Show version
  fstrace -h, --help         Show this help message

{BOLD}Options:{RESET}
  --json                  Output events as JSON
  --exclude <name>        Exclude a file or folder (can be repeated)

{BOLD}Examples:{RESET}
  fstrace ./project
  fstrace ./project --exclude node_modules --exclude .git
  fstrace ./project --json

{BOLD}Events:{RESET}
  {GREEN}[created]{RESET}   A file was created
  {YELLOW}[modified]{RESET}  A file was modified
  {RED}[deleted]{RESET}   A file was deleted
"
    );
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

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn emit(event: &Event, json: bool) {
    let path = match event {
        Event::Created { path } | Event::Modified { path } | Event::Deleted { path } => path,
    };
    if json {
        println!(
            "{{\"event\":\"{}\",\"path\":\"{path}\",\"timestamp\":{}}}",
            event.kind(),
            timestamp()
        );
    } else {
        let time = now();
        match event {
            Event::Created { .. } => {
                println!("{DIM}[{time}]{RESET} {GREEN}[created]{RESET}   {path}")
            }
            Event::Modified { .. } => {
                println!("{DIM}[{time}]{RESET} {YELLOW}[modified]{RESET}  {path}")
            }
            Event::Deleted { .. } => {
                println!("{DIM}[{time}]{RESET} {RED}[deleted]{RESET}   {path}")
            }
        }
    }
}

fn watch_file(path: &Path, json: bool) {
    if !json {
        println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
        println!("{DIM}Watching file: {}{RESET}\n", path.display());
    }

    let mut last = mtime(path);

    loop {
        thread::sleep(Duration::from_millis(500));

        let path_str = path.to_string_lossy();

        if !path.exists() {
            emit(
                &Event::Deleted {
                    path: path_str.into_owned(),
                },
                json,
            );
            break;
        }

        let current = mtime(path);
        if current != last {
            emit(
                &Event::Modified {
                    path: path_str.into_owned(),
                },
                json,
            );
            last = current;
        }
    }
}

fn watch_folder(path: &Path, json: bool, excludes: &[String]) {
    if !json {
        println!("{CYAN}{BOLD}fstrace{RESET} {DIM}v{VERSION}{RESET}");
        println!("{DIM}Watching folder: {}{RESET}\n", path.display());
    }

    let mut prev: HashMap<String, Option<SystemTime>> = snapshot(path, excludes);

    loop {
        thread::sleep(Duration::from_millis(500));

        let current = snapshot(path, excludes);
        for event in diff(&prev, &current) {
            emit(&event, json);
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

            let mut excludes: Vec<String> = Vec::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--exclude"
                    && let Some(val) = args.get(i + 1)
                {
                    excludes.push(val.clone());
                    i += 2;
                    continue;
                }
                i += 1;
            }

            let p = Path::new(path);
            if !p.exists() {
                eprintln!("{RED}Error:{RESET} path not found: {path}");
                process::exit(1);
            }
            if p.is_dir() {
                watch_folder(p, json, &excludes);
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
