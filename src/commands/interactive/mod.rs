mod command;
mod dialog;
mod runtime;
mod view;

#[allow(unused_imports)]
pub use command::InteractiveCommand;
#[allow(unused_imports)]
pub use runtime::{CrosstermEvents, run};

use std::path::PathBuf;

use crossterm::event::Event;
use ratatui::style::{Color, Modifier, Style};

pub trait EventSource {
    fn next(&mut self) -> color_eyre::Result<Event>;
}

#[derive(Clone, Debug)]
pub(crate) struct WorktreeEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

impl WorktreeEntry {
    pub(crate) fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    Worktrees,
    Actions,
    GlobalActions,
}

pub(crate) const GLOBAL_ACTIONS: [&str; 2] = ["Create worktree", "Cd to root dir"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Worktree(String),
    PrGithub(String),
    MergePrGithub {
        name: String,
        remove_local_branch: bool,
        remove_remote_branch: bool,
        remove_worktree: bool,
    },
    RepoRoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    OpenInEditor,
    Remove,
    PrGithub,
    MergePrGithub,
}

impl Action {
    pub(crate) const ALL: [Action; 5] = [
        Action::Open,
        Action::OpenInEditor,
        Action::Remove,
        Action::PrGithub,
        Action::MergePrGithub,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Action::Open => "Open",
            Action::OpenInEditor => "Open in Editor",
            Action::Remove => "Remove",
            Action::PrGithub => "PR (GitHub)",
            Action::MergePrGithub => "Merge PR (GitHub)",
        }
    }

    pub(crate) fn requires_selection(self) -> bool {
        matches!(
            self,
            Action::Open
                | Action::OpenInEditor
                | Action::Remove
                | Action::PrGithub
                | Action::MergePrGithub
        )
    }

    pub(crate) fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StatusMessage {
    pub(crate) text: String,
    pub(crate) kind: StatusKind,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum StatusKind {
    Info,
    Error,
}

impl StatusMessage {
    pub(crate) fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Info,
        }
    }

    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Error,
        }
    }

    pub(crate) fn style(&self) -> Style {
        match self.kind {
            StatusKind::Info => Style::default().fg(Color::Gray),
            StatusKind::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

#[cfg(test)]
mod tests;
