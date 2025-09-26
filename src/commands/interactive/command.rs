use std::path::PathBuf;

use color_eyre::{Result, eyre::WrapErr};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::Backend, widgets::ListState};

use super::{
    Action, EventSource, Focus, StatusMessage, WorktreeEntry,
    dialog::{CreateDialog, CreateDialogFocus, Dialog},
    view::{DetailData, DialogView, Snapshot},
};

pub struct InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    pub(crate) terminal: Terminal<B>,
    events: E,
    worktrees_dir: PathBuf,
    pub(crate) worktrees: Vec<WorktreeEntry>,
    pub(crate) selected: Option<usize>,
    pub(crate) focus: Focus,
    pub(crate) action_selected: usize,
    pub(crate) global_action_selected: usize,
    pub(crate) branches: Vec<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) status: Option<StatusMessage>,
    pub(crate) dialog: Option<Dialog>,
}

impl<B, E> InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    pub fn new(
        terminal: Terminal<B>,
        events: E,
        worktrees_dir: PathBuf,
        worktrees: Vec<WorktreeEntry>,
        mut branches: Vec<String>,
        default_branch: Option<String>,
    ) -> Self {
        let selected = if worktrees.is_empty() { None } else { Some(0) };

        branches.sort();
        branches.dedup();

        Self {
            terminal,
            events,
            worktrees_dir,
            worktrees,
            selected,
            focus: Focus::Worktrees,
            action_selected: 0,
            global_action_selected: 0,
            branches,
            default_branch,
            status: None,
            dialog: None,
        }
    }

    pub fn run<F, G>(mut self, mut on_remove: F, mut on_create: G) -> Result<Option<String>>
    where
        F: FnMut(&str) -> Result<()>,
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        self.terminal
            .hide_cursor()
            .wrap_err("failed to hide cursor")?;

        let result = self.event_loop(&mut on_remove, &mut on_create);

        self.terminal
            .show_cursor()
            .wrap_err("failed to show cursor")?;

        result
    }

    fn event_loop<F, G>(&mut self, on_remove: &mut F, on_create: &mut G) -> Result<Option<String>>
    where
        F: FnMut(&str) -> Result<()>,
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        let mut state = ListState::default();
        self.sync_selection(&mut state);

        loop {
            let snapshot = self.snapshot();
            self.terminal
                .draw(|frame| snapshot.render(frame, &mut state))?;
            let event = self.events.next()?;

            match self.process_event(event, &mut state, on_remove, on_create)? {
                LoopControl::Continue => {}
                LoopControl::Exit(outcome) => return Ok(outcome),
            }
        }
    }

    fn process_event<F, G>(
        &mut self,
        event: Event,
        state: &mut ListState,
        on_remove: &mut F,
        on_create: &mut G,
    ) -> Result<LoopControl>
    where
        F: FnMut(&str) -> Result<()>,
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        if let Some(dialog) = self.dialog.clone() {
            match dialog {
                Dialog::ConfirmRemove { index } => {
                    if let Event::Key(key) = event {
                        if key.kind == KeyEventKind::Press {
                            self.handle_confirm(index, key.code, state, on_remove)?;
                        }
                    }
                    return Ok(LoopControl::Continue);
                }
                Dialog::Info { .. } => {
                    if let Event::Key(key) = event {
                        if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter {
                            self.dialog = None;
                        }
                    }
                    return Ok(LoopControl::Continue);
                }
                Dialog::Create(_) => {
                    if let Event::Key(key) = event {
                        if key.kind == KeyEventKind::Press {
                            self.handle_create_key(key, state, on_create)?;
                        }
                    }
                    return Ok(LoopControl::Continue);
                }
            }
        }

        let Event::Key(key) = event else {
            return Ok(LoopControl::Continue);
        };

        if key.kind != KeyEventKind::Press {
            return Ok(LoopControl::Continue);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Ok(LoopControl::Exit(None)),
            KeyCode::Tab | KeyCode::BackTab => {
                if key.code == KeyCode::Tab {
                    self.focus = self.focus.next();
                } else {
                    self.focus = self.focus.prev();
                }
                Ok(LoopControl::Continue)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.handle_up(state);
                Ok(LoopControl::Continue)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.handle_down(state);
                Ok(LoopControl::Continue)
            }
            KeyCode::Left => {
                match self.focus {
                    Focus::Actions => self.move_action(-1),
                    Focus::GlobalActions => self.move_global_action(-1),
                    Focus::Worktrees => {}
                }
                Ok(LoopControl::Continue)
            }
            KeyCode::Right => {
                match self.focus {
                    Focus::Actions => self.move_action(1),
                    Focus::GlobalActions => self.move_global_action(1),
                    Focus::Worktrees => {}
                }
                Ok(LoopControl::Continue)
            }
            KeyCode::Enter => self.handle_enter(),
            _ => Ok(LoopControl::Continue),
        }
    }

    fn handle_enter(&mut self) -> Result<LoopControl> {
        match self.focus {
            Focus::Worktrees => {
                if let Some(index) = self.selected {
                    return Ok(LoopControl::Exit(
                        self.worktrees.get(index).map(|entry| entry.name.clone()),
                    ));
                }
            }
            Focus::Actions => {
                let action = Action::from_index(self.action_selected);
                match action {
                    Action::Open => {
                        if let Some(entry) = self.current_entry() {
                            return Ok(LoopControl::Exit(Some(entry.name.clone())));
                        }
                        self.status = Some(StatusMessage::info("No worktree selected."));
                    }
                    Action::Remove => {
                        if let Some(index) = self.selected {
                            self.dialog = Some(Dialog::ConfirmRemove { index });
                        } else {
                            self.status =
                                Some(StatusMessage::info("No worktree selected to remove."));
                        }
                    }
                }
            }
            Focus::GlobalActions => {
                if self.global_action_selected == 0 {
                    let dialog =
                        CreateDialog::new(&self.branches, &self.worktrees, self.default_branch());
                    self.dialog = Some(Dialog::Create(dialog));
                }
            }
        }

        Ok(LoopControl::Continue)
    }

    fn handle_confirm<F>(
        &mut self,
        index: usize,
        code: KeyCode,
        state: &mut ListState,
        on_remove: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&str) -> Result<()>,
    {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(entry) = self.worktrees.get(index).cloned() {
                    match on_remove(&entry.name) {
                        Ok(()) => {
                            self.worktrees.remove(index);
                            let removal_dir = entry
                                .path
                                .parent()
                                .map(|parent| parent.display().to_string())
                                .unwrap_or_else(|| entry.path.display().to_string());
                            let message = format!(
                                "Removed worktree `{}` from `{}`.",
                                entry.name, removal_dir
                            );
                            self.selected = None;
                            self.focus = Focus::Worktrees;
                            self.sync_selection(state);
                            self.status = None;
                            self.dialog = Some(Dialog::Info { message });
                            return Ok(());
                        }
                        Err(err) => {
                            self.status = Some(StatusMessage::error(format!(
                                "Failed to remove `{}`: {err}",
                                entry.name
                            )));
                            self.dialog = None;
                            return Ok(());
                        }
                    }
                }
                self.dialog = None;
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.status = Some(StatusMessage::info("Removal cancelled."));
                self.dialog = None;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_create_key<G>(
        &mut self,
        key: KeyEvent,
        state: &mut ListState,
        on_create: &mut G,
    ) -> Result<()>
    where
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        let mut close_dialog = false;
        let mut status_message: Option<StatusMessage> = None;
        let mut submit_requested = false;

        {
            let Some(dialog) = self.dialog.as_mut().and_then(|dialog| {
                if let Dialog::Create(dialog) = dialog {
                    Some(dialog)
                } else {
                    None
                }
            }) else {
                return Ok(());
            };

            let modifiers = key.modifiers;

            if key.code == KeyCode::Esc {
                close_dialog = true;
                status_message = Some(StatusMessage::info("Creation cancelled."));
                self.focus = Focus::Worktrees;
                dialog.error = None;
                dialog.name_input.clear();
            } else if key.code == KeyCode::Tab {
                dialog.focus_next();
                return Ok(());
            } else if key.code == KeyCode::BackTab {
                dialog.focus_prev();
                return Ok(());
            }

            if close_dialog {
                // Skip additional handling when dialog marked to close.
            } else {
                match dialog.focus {
                    CreateDialogFocus::Name => match key.code {
                        KeyCode::Char(c)
                            if !modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
                            dialog.name_input.push(c);
                            dialog.error = None;
                        }
                        KeyCode::Backspace => {
                            dialog.name_input.pop();
                            dialog.error = None;
                        }
                        KeyCode::Enter => {
                            dialog.focus = CreateDialogFocus::Base;
                        }
                        _ => {}
                    },
                    CreateDialogFocus::Base => match key.code {
                        KeyCode::Up | KeyCode::Char('k') => dialog.move_base(-1),
                        KeyCode::Down | KeyCode::Char('j') => dialog.move_base(1),
                        KeyCode::Enter => {
                            dialog.focus = CreateDialogFocus::Buttons;
                            dialog.buttons_selected = 0;
                        }
                        _ => {}
                    },
                    CreateDialogFocus::Buttons => match key.code {
                        KeyCode::Left => {
                            if dialog.buttons_selected > 0 {
                                dialog.buttons_selected -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if dialog.buttons_selected < 1 {
                                dialog.buttons_selected += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if dialog.buttons_selected == 0 {
                                submit_requested = true;
                            } else {
                                close_dialog = true;
                                status_message = Some(StatusMessage::info("Creation cancelled."));
                                self.focus = Focus::Worktrees;
                            }
                        }
                        _ => {}
                    },
                }
            }
        }

        if submit_requested {
            if let Some((name, base_label)) = self.perform_create_submission(state, on_create)? {
                close_dialog = true;
                status_message = Some(StatusMessage::info(format!(
                    "Created `{}` from {}",
                    name, base_label
                )));
            }
        }

        if close_dialog {
            self.dialog = None;
            self.focus = Focus::Worktrees;
            self.status = status_message;
        }

        Ok(())
    }

    fn submit_create<G>(
        &mut self,
        dialog: &mut CreateDialog,
        state: &mut ListState,
        on_create: &mut G,
    ) -> Result<Option<(String, String)>>
    where
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        dialog.error = None;

        let name_trimmed = dialog.name_input.trim();
        if name_trimmed.is_empty() {
            dialog.error = Some("Worktree name cannot be empty.".into());
            dialog.focus = CreateDialogFocus::Name;
            return Ok(None);
        }

        if self
            .worktrees
            .iter()
            .any(|entry| entry.name == name_trimmed)
        {
            dialog.error = Some(format!("Worktree `{}` already exists.", name_trimmed));
            dialog.focus = CreateDialogFocus::Name;
            return Ok(None);
        }

        let base_option = dialog.base_option();
        let base_value = base_option.and_then(|opt| opt.value.as_deref());
        let base_label = base_option
            .map(|opt| opt.label.clone())
            .unwrap_or_else(|| "HEAD".into());

        if let Err(err) = on_create(name_trimmed, base_value) {
            dialog.error = Some(err.to_string());
            dialog.focus = CreateDialogFocus::Name;
            return Ok(None);
        }

        let name_owned = name_trimmed.to_string();

        if !self.branches.iter().any(|branch| branch == &name_owned) {
            self.branches.push(name_owned.clone());
            self.branches.sort();
            self.branches.dedup();
        }

        let path = self.worktrees_dir.join(&name_owned);
        self.worktrees
            .push(WorktreeEntry::new(name_owned.clone(), path));
        self.worktrees.sort_by(|a, b| a.name.cmp(&b.name));
        self.selected = self
            .worktrees
            .iter()
            .position(|entry| entry.name == name_owned);
        self.focus = Focus::Worktrees;
        self.global_action_selected = 0;
        self.sync_selection(state);

        Ok(Some((name_owned, base_label)))
    }

    fn perform_create_submission<G>(
        &mut self,
        state: &mut ListState,
        on_create: &mut G,
    ) -> Result<Option<(String, String)>>
    where
        G: FnMut(&str, Option<&str>) -> Result<()>,
    {
        if let Some(Dialog::Create(mut dialog)) = self.dialog.take() {
            let outcome = self.submit_create(&mut dialog, state, on_create)?;

            if outcome.is_none() {
                self.dialog = Some(Dialog::Create(dialog));
            }

            Ok(outcome)
        } else {
            Ok(None)
        }
    }

    fn handle_up(&mut self, state: &mut ListState) {
        match self.focus {
            Focus::Worktrees => {
                if self.worktrees.is_empty() {
                    return;
                }
                let next = match self.selected {
                    Some(0) => Some(self.worktrees.len() - 1),
                    Some(idx) => Some(idx - 1),
                    None => Some(self.worktrees.len() - 1),
                };
                self.selected = next;
                self.sync_selection(state);
            }
            Focus::Actions => self.move_action(-1),
            Focus::GlobalActions => self.move_global_action(-1),
        }
    }

    fn handle_down(&mut self, state: &mut ListState) {
        match self.focus {
            Focus::Worktrees => {
                if self.worktrees.is_empty() {
                    return;
                }
                let next = match self.selected {
                    Some(idx) => (idx + 1) % self.worktrees.len(),
                    None => 0,
                };
                self.selected = Some(next);
                self.sync_selection(state);
            }
            Focus::Actions => self.move_action(1),
            Focus::GlobalActions => self.move_global_action(1),
        }
    }

    fn move_action(&mut self, delta: isize) {
        let len = Action::ALL.len() as isize;
        let current = self.action_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.action_selected = next as usize;
    }

    fn move_global_action(&mut self, delta: isize) {
        let len = super::GLOBAL_ACTIONS.len() as isize;
        if len == 0 {
            return;
        }
        let current = self.global_action_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.global_action_selected = next as usize;
    }

    fn current_entry(&self) -> Option<&WorktreeEntry> {
        self.selected.and_then(|idx| self.worktrees.get(idx))
    }

    fn sync_selection(&mut self, state: &mut ListState) {
        if let Some(idx) = self.selected {
            if self.worktrees.is_empty() {
                self.selected = None;
            } else if idx >= self.worktrees.len() {
                self.selected = Some(self.worktrees.len() - 1);
            }
        }

        if self.worktrees.is_empty() {
            self.selected = None;
        }

        state.select(self.selected);
    }

    fn default_branch(&self) -> Option<&str> {
        self.default_branch.as_deref()
    }

    fn snapshot(&self) -> Snapshot {
        let items = self
            .worktrees
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();

        let detail = self.current_entry().map(|entry| DetailData {
            name: entry.name.clone(),
            path: entry.path.display().to_string(),
        });

        let dialog = match self.dialog.clone() {
            Some(Dialog::ConfirmRemove { index }) => {
                self.worktrees
                    .get(index)
                    .map(|entry| DialogView::ConfirmRemove {
                        name: entry.name.clone(),
                    })
            }
            Some(Dialog::Info { message }) => Some(DialogView::Info { message }),
            Some(Dialog::Create(dialog)) => Some(DialogView::Create(dialog.into())),
            None => None,
        };

        Snapshot::new(
            items,
            detail,
            self.focus,
            self.action_selected,
            self.global_action_selected,
            self.status.clone(),
            dialog,
            !self.worktrees.is_empty(),
        )
    }
}

enum LoopControl {
    Continue,
    Exit(Option<String>),
}
