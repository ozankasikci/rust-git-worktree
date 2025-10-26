# rsworktree

[![Codecov](https://codecov.io/gh/ozankasikci/rust-git-worktree/branch/master/graph/badge.svg)](https://codecov.io/gh/ozankasikci/rust-git-worktree)

`rsworktree` is a Rust CLI for managing Git worktrees in a single repo-local directory (`.rsworktree`). It provides a focused, ergonomic workflow for creating, jumping into, listing, and removing worktrees without leaving the terminal.

## Table of Contents

- [Interactive mode](#interactive-mode)
- [CLI commands](#cli-commands)
  - [`rsworktree create`](#rsworktree-create)
  - [`rsworktree cd`](#rsworktree-cd)
  - [`rsworktree ls`](#rsworktree-ls)
  - [`rsworktree rm`](#rsworktree-rm)
  - [`rsworktree pr-github`](#rsworktree-pr-github)
  - [`rsworktree merge-pr-github`](#rsworktree-merge-pr-github)
  - [`rsworktree worktree open-editor`](#rsworktree-worktree-open-editor)
- [Installation](#installation)
- [Environment](#environment)

## Interactive mode

- Open a terminal UI for browsing worktrees, focusing actions, and inspecting details without memorizing subcommands.
- Launch it with the `interactive` command: `rsworktree interactive` (shortcut: `rsworktree i`).
- Available actions include opening worktrees, launching editors, removing worktrees, creating PRs, and merging PRs without leaving the TUI.
- Use the **Open in Editor** action to launch the highlighted worktree in your configured editor (initial support covers `vim`, `cursor`, `webstorm`, and `rider`; see the quickstart for setup guidance).
- The merge flow lets you decide whether to keep the local branch, delete the remote branch, and clean up the worktree before exiting.
- ![Interactive mode screenshot](tapes/gifs/interactive-mode.gif)

## CLI commands

### `rsworktree create`

- Create a new worktree under `.rsworktree/<name>`. Also changes directory to the worktree.
- Demo: ![Create demo](tapes/gifs/create.gif)
- Options:
  - `--base <branch>` — branch from `<branch>` instead of the current git branch.

### `rsworktree cd`

- Spawn an interactive shell rooted in the named worktree.
- Demo: ![CD demo](tapes/gifs/cd.gif)
- Options:
  - `--print` — write the worktree path to stdout without spawning a shell.

### `rsworktree ls`

- List all worktrees tracked under `.rsworktree`, showing nested worktree paths.
- Demo: ![List demo](tapes/gifs/ls.gif)
- Options:
  - _(none)_

### `rsworktree rm`

- Remove the named worktree.
- Demo: ![Remove demo](tapes/gifs/rm.gif)
- Options:
  - `--force` — force removal, mirroring `git worktree remove --force`.

### `rsworktree pr-github`

- Push the worktree branch and invoke `gh pr create` for the current or named worktree.
- Demo: ![PR demo](tapes/gifs/pr_github.gif)
- Requires the [GitHub CLI](https://cli.github.com/) (`gh`) to be installed and on your `PATH`.
- Options:
  - `<name>` — optional explicit worktree to operate on; defaults to the current directory.
  - `--remove` — delete the remote branch after a successful merge.
  - `--no-push` — skip pushing the branch before creating the PR.
  - `--draft` — open the PR in draft mode.
  - `--fill` — let `gh pr create` auto-populate PR metadata.
  - `--web` — open the PR creation flow in a browser instead of filling via CLI.
  - `--reviewer <login>` — add one or more reviewers by GitHub login.
  - `-- <extra gh args>` — pass additional arguments through to `gh pr create`.

### `rsworktree merge-pr-github`

- Merge the open GitHub pull request for the current or named worktree using `gh pr merge`.
- Demo: ![Merge PR demo](tapes/gifs/merge_pr_github.gif)
- Requires the [GitHub CLI](https://cli.github.com/) (`gh`) to be installed and on your `PATH`.
- Options:
  - `<name>` — optional explicit worktree to operate on; defaults to the current directory.

### `rsworktree worktree open-editor`

- Open the specified worktree (or the current directory when omitted) in your configured editor.
- Editor resolution checks the rsworktree config first, then falls back to `$EDITOR` / `$VISUAL`. If no editor is configured, the command prints actionable guidance instead of failing.
- Initial support focuses on `vim`, `cursor`, `webstorm`, and `rider`. For setup instructions and troubleshooting, see `specs/002-i-want-to/quickstart.md`.

## Installation

Install from crates.io with:

```bash
cargo install rsworktree
```

On macOS you can install via Homebrew:

```bash
brew tap ozankasikci/tap
brew install rsworktree
```

After the binary is on your `PATH`, run `rsworktree --help` to explore the available commands.

## Environment

Set `RSWORKTREE_SHELL` to override the shell used by `rsworktree cd` (falls back to `$SHELL` or `/bin/sh`).
