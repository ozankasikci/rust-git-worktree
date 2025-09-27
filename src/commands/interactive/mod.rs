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

impl Focus {
    pub(crate) fn next(self) -> Self {
        match self {
            Focus::Worktrees => Focus::Actions,
            Focus::Actions => Focus::GlobalActions,
            Focus::GlobalActions => Focus::Worktrees,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Focus::Worktrees => Focus::GlobalActions,
            Focus::Actions => Focus::Worktrees,
            Focus::GlobalActions => Focus::Actions,
        }
    }
}

pub(crate) const GLOBAL_ACTIONS: [&str; 2] = ["Create worktree", "Cd to root dir"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Worktree(String),
    PrGithub(String),
    RepoRoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Remove,
    PrGithub,
}

impl Action {
    pub(crate) const ALL: [Action; 3] = [Action::Open, Action::Remove, Action::PrGithub];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Action::Open => "Open",
            Action::Remove => "Remove",
            Action::PrGithub => "PR (GitHub)",
        }
    }

    pub(crate) fn requires_selection(self) -> bool {
        matches!(self, Action::Open | Action::Remove | Action::PrGithub)
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
