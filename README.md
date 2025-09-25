# rsworktree

`rsworktree` is a Rust CLI for managing Git worktrees in a single repo-local directory (`.rsworktree`). It provides a focused, ergonomic workflow for creating, jumping into, listing, and removing worktrees without leaving the terminal.

## Commands

- `rsworktree create <name> [--base <branch>]`
  - Create a new worktree under `.rsworktree/<name>`, branching at `<name>` by default or from `--base` if provided.

- `rsworktree cd <name> [--print]`
  - Spawn an interactive shell rooted in the named worktree. Use `--print` to output the path instead.

- `rsworktree ls`
  - List all worktrees tracked under `.rsworktree`, showing nested worktree paths.

- `rsworktree rm <name> [--force]`
  - Remove the named worktree. Pass `--force` to mirror `git worktree remove --force` behavior.

## Environment

Set `RSWORKTREE_SHELL` to override the shell used by `rsworktree cd` (falls back to `$SHELL` or `/bin/sh`).

