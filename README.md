# ϵpistola

> [!WARNING]
> 🚧 **Extremely Work in Progress** 🚧
>
> This project is still in a very early and unstable stage. It might change drastically, break without notice, or never be finished.
>
> **Use at your own risk.**

A Rust-native HTTP API client for building, saving, and running API requests — a CLI and a desktop GUI, sharing the same collection format and orchestration engine.

## Features

- **Plain-text collections.** Requests, environments, and folder-level settings are TOML files (`<name>.req.toml`, `environments/<name>.toml`, `folder.toml`) meant to be committed to git and diffed/reviewed like code.
- **Variables and environments.** `{{var}}` interpolation via Jinja-style templating, layered from global user config → collection variables → named environment → request-level variables → CLI overrides.
- **Folder inheritance.** A `folder.toml` can set headers/auth for every request under a directory; requests inherit them unless they explicitly opt out.
- **Two ways to work.** A full `epistola` CLI (saved requests, environments, folders, history) and an ad-hoc httpie-style mode (`epistola GET https://example.com -H ... -q ...`) for one-off requests, with `--save` to promote one into the collection.
- **A GPUI desktop client** for browsing, editing, and running the same collections visually.

## Installation

There are no stable releases yet — only automated nightly builds (unstable, expect breakage). Grab one from the [Codeberg releases](https://codeberg.org/viniciusdof/epistola/releases) page, tagged `nightly-<date>-<sha>`:

| Platform | Formats |
| --- | --- |
| Linux | `.deb`, `.AppImage`, pacman package |
| macOS | `.dmg` |
| Windows | NSIS installer (`.exe`) |

Each release includes both the `epistola` CLI and the `epistola_gui` desktop client; Linux packages also install shell completions (bash/zsh/fish).

## Quick start

```sh
# Scaffold a new collection
epistola init my-api
cd my-api

# Create and run a saved request
epistola request new get-user
epistola run get-user.req.toml

# Or skip saving entirely — httpie-style ad-hoc requests
epistola GET https://example.com/users -H "Accept: application/json" -q page=2

# Promote an ad-hoc request into the collection
epistola GET https://example.com/users --save get-users
```

Run `epistola --help` or `epistola <command> --help` for full usage; top-level commands are `init`, `request`, `env`, `folder`, `run`, `history`, and `completions`.

## Development

Tooling is managed via [mise](https://mise.jdx.dev/); `just <recipe>` routes through it automatically.

```sh
just check      # fmt-check + lint + test + deny + shear (what the pre-push hook runs)
just test       # cargo test --all-features, all crates
just run -- GET https://example.com   # build and run the CLI
just run-gui    # run the GPUI desktop client
```

Run `just --list` for the full recipe list, or `just hooks-install` to set up the git hooks.

## License

This project is licensed under the MIT License. See [`LICENSE`](./LICENSE) for details.
