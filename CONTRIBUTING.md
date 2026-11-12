# Contributing to fstrace

Contributions are welcome, whether you are fixing a bug, improving documentation, or proposing a feature.

## Getting started

Fork the repository, clone your fork, and create a branch for your changes:

```sh
git clone https://github.com/<your-username>/fstrace.git
cd fstrace
git switch -c fix/describe-your-change
```

You will need a Rust toolchain that supports the 2024 edition.

Build the project and check the CLI:

```sh
cargo build
cargo run -- --help
```

## Making changes

Keep each contribution focused on a specific problem. For larger changes, open an issue first so the approach can be discussed.

fstrace is both a CLI and a library:

- Public API changes in `src/lib.rs` affect library users. Document them in rustdoc and mention them in the pull request.
- Snapshot behavior changes alter what `diff` reports. Add tests for the new cases.
- CLI behavior and flag changes need a README update.

Add regression tests for behavior changes. Use temporary directories in filesystem tests and remove them afterwards.

## Checking your work

Before opening a pull request, run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

Also run the CLI against a small temporary tree when changing watch or output behavior.

## Commit messages

Use Conventional Commits with a type and scope:

```text
type(scope): short description
```

Examples:

```text
feat(state): record entry kind and symlink targets
fix(walk): keep dangling links in snapshots
docs(readme): document the library example
test(diff): cover deletion ordering
chore(deps): update dependencies
```

Use a short, imperative description explaining what the commit changes.

## Pull requests

In your pull request, explain:

- What problem the change addresses.
- What changed and why.
- How you verified it.
- Any changes to the public API, snapshot behavior, or CLI output.

Link the relevant issue if one exists, and update documentation when behavior changes.

## Reporting bugs

Include the command or API call you ran, what you expected, and what happened instead. Share your operating system, fstrace version or commit, and a small reproduction when possible. Bug reports can use the issue form.

Remove private paths and sensitive file contents before sharing logs or examples.

## Suggesting features

Describe the use case and what you want to accomplish. Examples of the proposed CLI output or API shape make the request easier to discuss. Feature requests can use the issue form.

## License

By contributing, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
