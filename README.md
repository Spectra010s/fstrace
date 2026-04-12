# fstrace

A lightweight file system watcher for files and folders.

## Installation

**Linux / macOS:**
```sh
curl -fsSL https://spectra010s.github.io/fstrace/install.sh | sh
```

**Android (Termux):**
```sh
curl -fsSL https://spectra010s.github.io/fstrace/install.sh | sh
```

**Windows:**
```ps1
iex (irm https://spectra010s.github.io/fstrace/install.ps1)
```

**npm:**
```sh
npm install -g @hiverra/fstrace
```

**cargo:**
```sh
cargo install fstrace
```

**Build from source:**
```bash
git clone https://github.com/Spectra010s/fstrace.git
cd fstrace
cargo build --release
cp target/release/fstrace $PREFIX/bin/fstrace
```

---

## Usage

```bash
fstrace <path> [options]
```

**Watch a file:**
```bash
fstrace file.txt
```

**Watch a folder:**
```bash
fstrace ./myproject
```

**Exclude folders or files:**
```bash
fstrace ./myproject --exclude node_modules --exclude .git
```

**JSON output:**
```bash
fstrace ./myproject --json
```

**Combine flags:**
```bash
fstrace ./myproject --exclude node_modules --json
```

---

## Events

| Event      | Description         |
|------------|---------------------|
| `created`  | A file was created  |
| `modified` | A file was modified |
| `deleted`  | A file was deleted  |

---

## JSON Output

When using `--json`, each event is a single line of JSON:

```json
{"event":"modified","path":"/home/user/project/index.js","timestamp":1744392000}
```

Useful for consuming events in other tools or scripts:

```js
import { spawn } from "child_process";

const proc = spawn("fstrace", ["./project", "--json"]);

proc.stdout.on("data", (data) => {
  const event = JSON.parse(data);
  console.log(event.event, event.path);
});
```

---

## Options

| Flag                    | Description                          |
|-------------------------|--------------------------------------|
| `--exclude <name>`      | Exclude a file or folder (repeatable)|
| `--json`                | Output events as JSON                |
| `-v, --version`         | Show version                         |
| `-h, --help`            | Show help                            |

---

## License

MIT — see the [LICENSE](LICENSE) file for details.

---

Written by [Spectra010s](https://spectra010s.vercel.app)

