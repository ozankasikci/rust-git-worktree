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

## Recording demos

The repository includes [VHS](https://github.com/charmbracelet/vhs) tapes under `tapes/` for each primary command. Install VHS and run, for example:

```bash
vhs < tapes/create.tape
vhs < tapes/cd.tape
vhs < tapes/ls.tape
vhs < tapes/rm.tape
```

Each command generates a matching `*.gif` demo. The tapes assume the repo contains no existing `demo` worktree; re-run `rsworktree rm demo --force` if you need a clean slate before recording.

You can also run `tapes/scripts/reset_tapes.sh` to delete the demo worktrees and branches used in the recordings (`demo-create`, `demo-cd`, `demo-ls`, `demo-rm`). To regenerate every GIF in one shot, use `tapes/scripts/run_all.sh` (requires VHS to be installed).

### Pre-rendered demos

![Create demo](tapes/gifs/create.gif)
![CD demo](tapes/gifs/cd.gif)
![List demo](tapes/gifs/ls.gif)
![Remove demo](tapes/gifs/rm.gif)
