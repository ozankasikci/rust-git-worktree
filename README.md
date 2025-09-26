# rsworktree

`rsworktree` is a Rust CLI for managing Git worktrees in a single repo-local directory (`.rsworktree`). It provides a focused, ergonomic workflow for creating, jumping into, listing, and removing worktrees without leaving the terminal.

## Commands

- `rsworktree create <name> [--base <branch>]`
  - Create a new worktree under `.rsworktree/<name>`, branching at `<name>` by default or from `--base` if provided.
  - Demo: ![Create demo](tapes/gifs/create.gif)

- `rsworktree cd <name> [--print]`
  - Spawn an interactive shell rooted in the named worktree. Use `--print` to output the path instead.
  - Demo: ![CD demo](tapes/gifs/cd.gif)

- `rsworktree ls`
  - List all worktrees tracked under `.rsworktree`, showing nested worktree paths.
  - Demo: ![List demo](tapes/gifs/ls.gif)

- `rsworktree rm <name> [--force]`
  - Remove the named worktree. Pass `--force` to mirror `git worktree remove --force` behavior.
  - Demo: ![Remove demo](tapes/gifs/rm.gif)

- `rsworktree pr-github [<name>] [--no-push] [--draft] [--fill] [--web] [--reviewer <login> ...] [-- <extra gh args>]`
  - Push the worktree branch (unless `--no-push`) and invoke `gh pr create` with the provided options. When `<name>` is omitted, the command uses the current `.rsworktree/<name>` directory. Because `rsworktree` calls `gh` non-interactively, pass `--fill` or provide `--title/--body` (or open `--web`) so the PR metadata can be supplied.
  - Requires the [GitHub CLI](https://cli.github.com/) (`gh`) to be installed and on your `PATH`.

## Environment

Set `RSWORKTREE_SHELL` to override the shell used by `rsworktree cd` (falls back to `$SHELL` or `/bin/sh`).

## Installation

Install from crates.io with:

```bash
cargo install rsworktree
```

After the binary is on your `PATH`, run `rsworktree --help` to explore the available commands.
