# rsworktree

[![Codecov](https://codecov.io/gh/ozankasikci/rust-git-worktree/branch/master/graph/badge.svg)](https://codecov.io/gh/ozankasikci/rust-git-worktree)

`rsworktree` is a Rust CLI for managing Git worktrees in a single repo-local directory (`.rsworktree`). It provides a focused, ergonomic workflow for creating, jumping into, listing, and removing worktrees without leaving the terminal.

## Table of Contents

- [Commands](#commands)
  - [`rsworktree create`](#rsworktree-create)
  - [`rsworktree cd`](#rsworktree-cd)
  - [`rsworktree ls`](#rsworktree-ls)
  - [`rsworktree rm`](#rsworktree-rm)
  - [`rsworktree pr-github`](#rsworktree-pr-github)
- [Installation](#installation)
- [Environment](#environment)

## Commands

### `rsworktree create`

- Create a new worktree under `.rsworktree/<name>`, branching at `<name>` by default or from `--base` if provided.
- Demo: ![Create demo](tapes/gifs/create.gif)
- Options:
  - `--base <branch>` — branch from `<branch>` instead of `<name>`.

### `rsworktree cd`

- Spawn an interactive shell rooted in the named worktree. Use `--print` to output the path instead.
- Demo: ![CD demo](tapes/gifs/cd.gif)
- Options:
  - `--print` — write the worktree path to stdout without spawning a shell.

### `rsworktree ls`

- List all worktrees tracked under `.rsworktree`, showing nested worktree paths.
- Demo: ![List demo](tapes/gifs/ls.gif)
- Options:
  - _(none)_

### `rsworktree rm`

- Remove the named worktree. Pass `--force` to mirror `git worktree remove --force` behavior.
- Demo: ![Remove demo](tapes/gifs/rm.gif)
- Options:
  - `--force` — force removal, mirroring `git worktree remove --force`.

### `rsworktree pr-github`

- Push the worktree branch (unless `--no-push`) and invoke `gh pr create` with the provided options. When `<name>` is omitted, the command uses the current `.rsworktree/<name>` directory. If you don’t supply PR metadata flags, `rsworktree` automatically adds `--fill`; you can pass `--title/--body` or `--web` to override that behaviour.
- Demo: ![PR demo](tapes/gifs/pr_github.gif)
- Requires the [GitHub CLI](https://cli.github.com/) (`gh`) to be installed and on your `PATH`.
- Options:
  - `<name>` — optional explicit worktree to operate on; defaults to the current directory.
  - `--no-push` — skip pushing the branch before creating the PR.
  - `--draft` — open the PR in draft mode.
  - `--fill` — let `gh pr create` auto-populate PR metadata.
  - `--web` — open the PR creation flow in a browser instead of filling via CLI.
  - `--reviewer <login>` — add one or more reviewers by GitHub login.
  - `-- <extra gh args>` — pass additional arguments through to `gh pr create`.

## Installation

Install from crates.io with:

```bash
cargo install rsworktree
```

After the binary is on your `PATH`, run `rsworktree --help` to explore the available commands.

## Environment

Set `RSWORKTREE_SHELL` to override the shell used by `rsworktree cd` (falls back to `$SHELL` or `/bin/sh`).
