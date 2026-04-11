# fstrace

A lightweight file system watcher for files and folders.

## Installation

```bash
git clone https://github.com/Spectra010s/fstrace.git
cd fstrace
cargo build --release
```

Then move the binary to your PATH:

```bash
cp target/release/fstrace $PREFIX/bin/fstrace
```

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

**JSON output (for programmatic use):**

```bash
fstrace ./myproject --json
```

## Events

| Event      | Description              |
|------------|--------------------------|
| `created`  | A file was created       |
| `modified` | A file was modified      |
| `deleted`  | A file was deleted       |

## JSON Output

When using `--json`, each event is printed as a single line of JSON:

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

## Options

| Flag             | Description       |
|------------------|-------------------|
| `--json`         | Output as JSON    |
| `-v, --version`  | Show version      |
| `-h, --help`     | Show help         |

## License

MIT — see the [LICENSE](LICENSE) file for details.

---

Written by [Spectra010s](https://spectra010s.vercel.app)

