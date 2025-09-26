use std::{io, path::PathBuf};

use color_eyre::eyre::{self, WrapErr};
use crossterm::{
    event::{Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    Repo,
    commands::{
        cd::CdCommand,
        list::{find_worktrees, format_worktree},
        rm::RemoveCommand,
    },
};

pub trait EventSource {
    fn next(&mut self) -> color_eyre::Result<Event>;
}

#[derive(Clone, Debug)]
pub(crate) struct WorktreeEntry {
    name: String,
    path: PathBuf,
}

impl WorktreeEntry {
    fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Worktrees,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Open,
    Remove,
}

impl Action {
    const ALL: [Action; 2] = [Action::Open, Action::Remove];

    fn label(self) -> &'static str {
        match self {
            Action::Open => "Open",
            Action::Remove => "Remove",
        }
    }

    fn requires_selection(self) -> bool {
        match self {
            Action::Open | Action::Remove => true,
        }
    }

    fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }
}

#[derive(Clone, Debug)]
enum Dialog {
    ConfirmRemove { index: usize },
    Info { message: String },
}

#[derive(Clone, Debug)]
struct StatusMessage {
    text: String,
    kind: StatusKind,
}

#[derive(Clone, Copy, Debug)]
enum StatusKind {
    Info,
    Error,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Info,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Error,
        }
    }

    fn style(&self) -> Style {
        match self.kind {
            StatusKind::Info => Style::default().fg(Color::Gray),
            StatusKind::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

#[derive(Clone, Debug)]
struct DetailData {
    name: String,
    path: String,
}

#[derive(Clone, Debug)]
enum DialogView {
    ConfirmRemove { name: String },
    Info { message: String },
}

#[derive(Clone, Debug)]
struct SnapshotData {
    items: Vec<String>,
    detail: Option<DetailData>,
    focus: Focus,
    action_selected: usize,
    status: Option<StatusMessage>,
    dialog: Option<DialogView>,
    has_worktrees: bool,
}

impl SnapshotData {
    fn capture<B, E>(command: &InteractiveCommand<B, E>) -> Self
    where
        B: Backend,
        E: EventSource,
    {
        let items = command
            .worktrees
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();

        let detail = command.current_entry().map(|entry| DetailData {
            name: entry.name.clone(),
            path: entry.path.display().to_string(),
        });

        let dialog = match command.dialog.clone() {
            Some(Dialog::ConfirmRemove { index }) => {
                command
                    .worktrees
                    .get(index)
                    .map(|entry| DialogView::ConfirmRemove {
                        name: entry.name.clone(),
                    })
            }
            Some(Dialog::Info { message }) => Some(DialogView::Info { message }),
            None => None,
        };

        Self {
            items,
            detail,
            focus: command.focus,
            action_selected: command.action_selected,
            status: command.status.clone(),
            dialog,
            has_worktrees: !command.worktrees.is_empty(),
        }
    }

    fn render(&self, frame: &mut Frame, state: &mut ListState) {
        let size = frame.size();
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(size);

        self.render_list(frame, chunks[0], state);
        self.render_details(frame, chunks[1]);

        if let Some(dialog) = &self.dialog {
            match dialog {
                DialogView::ConfirmRemove { name } => self.render_confirmation(frame, size, name),
                DialogView::Info { message } => self.render_info(frame, size, message),
            }
        }
    }

    fn render_list(&self, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let items: Vec<ListItem> = if self.items.is_empty() {
            vec![ListItem::new("(no worktrees)")]
        } else {
            self.items.iter().cloned().map(ListItem::new).collect()
        };

        let list = List::new(items)
            .block(Block::default().title("Worktrees").borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(self.list_highlight_style());

        frame.render_stateful_widget(list, area, state);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let detail_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area);

        let mut lines = Vec::new();
        if let Some(detail) = &self.detail {
            lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(detail.name.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(detail.path.clone()),
            ]));
        } else {
            lines.push(Line::from("No worktree selected."));
        }

        lines.push(Line::from(""));
        if let Some(status) = &self.status {
            lines.push(Line::from(Span::styled(
                status.text.clone(),
                status.style(),
            )));
        } else {
            lines.push(Line::from("Use Tab to focus actions. Esc exits."));
        }

        let info =
            Paragraph::new(lines).block(Block::default().title("Details").borders(Borders::ALL));
        frame.render_widget(info, detail_chunks[0]);

        let mut spans = Vec::new();
        for (idx, action) in Action::ALL.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw("  "));
            }

            let mut style = Style::default();
            if action.requires_selection() && !self.has_worktrees {
                style = style.add_modifier(Modifier::DIM);
            }

            if self.focus == Focus::Actions && self.action_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            spans.push(Span::styled(format!("[{}]", action.label()), style));
        }

        let actions = Paragraph::new(Line::from(spans))
            .block(Block::default().title("Actions").borders(Borders::ALL));
        frame.render_widget(actions, detail_chunks[1]);
    }

    fn render_confirmation(&self, frame: &mut Frame, area: Rect, name: &str) {
        let popup_area = centered_rect(60, 30, area);
        frame.render_widget(Clear, popup_area);

        let lines = vec![
            Line::from(format!("Remove `{}`?", name)),
            Line::from("Press Y/Enter to confirm or Esc to cancel."),
        ];

        let popup = Paragraph::new(lines).block(
            Block::default()
                .title("Confirm removal")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
        frame.render_widget(popup, popup_area);
    }

    fn render_info(&self, frame: &mut Frame, area: Rect, message: &str) {
        let popup_area = centered_rect(60, 30, area);
        frame.render_widget(Clear, popup_area);

        let lines = vec![
            Line::from(message.to_owned()),
            Line::from(""),
            Line::from(Span::styled(
                "[ OK ]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Line::from("Press Enter to continue."),
        ];

        let popup = Paragraph::new(lines).block(
            Block::default()
                .title("Complete")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
        frame.render_widget(popup, popup_area);
    }

    fn list_highlight_style(&self) -> Style {
        match self.focus {
            Focus::Worktrees => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Focus::Actions => Style::default().add_modifier(Modifier::DIM),
        }
    }
}

pub struct InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    terminal: Terminal<B>,
    events: E,
    worktrees: Vec<WorktreeEntry>,
    selected: Option<usize>,
    focus: Focus,
    action_selected: usize,
    status: Option<StatusMessage>,
    dialog: Option<Dialog>,
}

impl<B, E> InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    pub fn new(terminal: Terminal<B>, events: E, worktrees: Vec<WorktreeEntry>) -> Self {
        let selected = if worktrees.is_empty() { None } else { Some(0) };

        Self {
            terminal,
            events,
            worktrees,
            selected,
            focus: Focus::Worktrees,
            action_selected: 0,
            status: None,
            dialog: None,
        }
    }

    pub fn run<F>(mut self, mut on_remove: F) -> color_eyre::Result<Option<String>>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
    {
        self.terminal
            .hide_cursor()
            .wrap_err("failed to hide cursor")?;

        let result = self.event_loop(&mut on_remove);

        self.terminal
            .show_cursor()
            .wrap_err("failed to show cursor")?;

        result
    }

    fn event_loop<F>(&mut self, on_remove: &mut F) -> color_eyre::Result<Option<String>>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
    {
        let mut state = ListState::default();
        self.sync_selection(&mut state);

        loop {
            let snapshot = SnapshotData::capture(self);
            self.terminal
                .draw(|frame| snapshot.render(frame, &mut state))?;
            let event = self.events.next()?;

            match self.process_event(event, &mut state, on_remove)? {
                LoopControl::Continue => {}
                LoopControl::Exit(outcome) => return Ok(outcome),
            }
        }
    }

    fn process_event<F>(
        &mut self,
        event: Event,
        state: &mut ListState,
        on_remove: &mut F,
    ) -> color_eyre::Result<LoopControl>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
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
                self.toggle_focus();
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
                self.move_action(-1);
                Ok(LoopControl::Continue)
            }
            KeyCode::Right => {
                self.move_action(1);
                Ok(LoopControl::Continue)
            }
            KeyCode::Enter => self.handle_enter(),
            _ => Ok(LoopControl::Continue),
        }
    }

    fn handle_enter(&mut self) -> color_eyre::Result<LoopControl> {
        match self.focus {
            Focus::Worktrees => {
                if let Some(entry) = self.current_entry() {
                    return Ok(LoopControl::Exit(Some(entry.name.clone())));
                }
                self.status = Some(StatusMessage::info("No worktree to open."));
                Ok(LoopControl::Continue)
            }
            Focus::Actions => {
                let action = Action::from_index(self.action_selected);
                self.execute_action(action)
            }
        }
    }

    fn execute_action(&mut self, action: Action) -> color_eyre::Result<LoopControl> {
        if action.requires_selection() && self.selected.is_none() {
            self.status = Some(StatusMessage::info("No worktree selected."));
            return Ok(LoopControl::Continue);
        }

        match action {
            Action::Open => {
                let entry = self
                    .current_entry()
                    .expect("checked selection before executing open");
                Ok(LoopControl::Exit(Some(entry.name.clone())))
            }
            Action::Remove => {
                let index = self
                    .selected
                    .expect("checked selection before executing remove");
                self.dialog = Some(Dialog::ConfirmRemove { index });
                Ok(LoopControl::Continue)
            }
        }
    }

    fn handle_confirm<F>(
        &mut self,
        index: usize,
        code: KeyCode,
        state: &mut ListState,
        on_remove: &mut F,
    ) -> color_eyre::Result<()>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
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
        }
    }

    fn move_action(&mut self, delta: isize) {
        let len = Action::ALL.len() as isize;
        let current = self.action_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.action_selected = next as usize;
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Worktrees => Focus::Actions,
            Focus::Actions => Focus::Worktrees,
        };
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
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(area);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(horizontal[1]);

    vertical[1]
}

enum LoopControl {
    Continue,
    Exit(Option<String>),
}

pub struct CrosstermEvents;

impl Default for CrosstermEvents {
    fn default() -> Self {
        Self
    }
}

impl EventSource for CrosstermEvents {
    fn next(&mut self) -> color_eyre::Result<Event> {
        crossterm::event::read().wrap_err("failed to read terminal event")
    }
}

pub fn run(repo: &Repo) -> color_eyre::Result<()> {
    let worktrees_dir = repo.ensure_worktrees_dir()?;
    let raw_entries = find_worktrees(&worktrees_dir)?;
    let worktrees = raw_entries
        .into_iter()
        .map(|path| {
            let display = format_worktree(&path);
            WorktreeEntry::new(display, worktrees_dir.join(&path))
        })
        .collect::<Vec<_>>();

    enable_raw_mode().wrap_err("failed to enable raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen).wrap_err("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).wrap_err("failed to initialize terminal")?;
    let events = CrosstermEvents::default();

    let command = InteractiveCommand::new(terminal, events, worktrees);
    let result = command.run(|name| {
        let command = RemoveCommand::new(name.to_owned(), false).with_quiet(true);
        command.execute(repo)
    });
    let cleanup_result = cleanup_terminal();

    let selection = match (result, cleanup_result) {
        (Ok(selection), Ok(())) => selection,
        (Err(run_err), Ok(())) => return Err(run_err),
        (Ok(_), Err(cleanup_err)) => return Err(cleanup_err),
        (Err(run_err), Err(cleanup_err)) => {
            return Err(eyre::eyre!(
                "interactive session failed ({run_err}); cleanup failed: {cleanup_err}"
            ));
        }
    };

    if let Some(name) = selection {
        let command = CdCommand::new(name, false);
        command.execute(repo)?;
    }

    Ok(())
}

fn cleanup_terminal() -> color_eyre::Result<()> {
    disable_raw_mode().wrap_err("failed to disable raw mode")?;
    execute!(io::stdout(), LeaveAlternateScreen).wrap_err("failed to leave alternate screen")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    use color_eyre::{Result, eyre};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    struct StubEvents {
        events: VecDeque<Event>,
    }

    impl StubEvents {
        fn new(events: Vec<Event>) -> Self {
            Self {
                events: events.into_iter().collect(),
            }
        }
    }

    impl EventSource for StubEvents {
        fn next(&mut self) -> color_eyre::Result<Event> {
            self.events
                .pop_front()
                .ok_or_else(|| eyre::eyre!("no more events"))
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn char_key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn entries(names: &[&str]) -> Vec<WorktreeEntry> {
        names
            .iter()
            .map(|name| WorktreeEntry::new((*name).into(), PathBuf::from(format!("/tmp/{name}"))))
            .collect()
    }

    #[test]
    fn returns_first_worktree_when_enter_pressed_immediately() -> Result<()> {
        let backend = TestBackend::new(40, 10);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![key(KeyCode::Enter)]);
        let worktrees = entries(&["alpha", "beta"]);
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let selection = command.run(|_| Ok(()))?.expect("expected selection");
        assert_eq!(selection, "alpha");

        Ok(())
    }

    #[test]
    fn navigates_down_before_selecting() -> Result<()> {
        let backend = TestBackend::new(40, 10);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![key(KeyCode::Down), key(KeyCode::Enter)]);
        let worktrees = entries(&["alpha", "beta", "gamma"]);
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let selection = command.run(|_| Ok(()))?.expect("expected selection");
        assert_eq!(selection, "beta");

        Ok(())
    }

    #[test]
    fn tabbing_to_actions_removes_selected_worktree() -> Result<()> {
        let backend = TestBackend::new(40, 12);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![
            key(KeyCode::Down),
            key(KeyCode::Tab),
            key(KeyCode::Down),
            key(KeyCode::Enter),
            char_key('y'),
            key(KeyCode::Enter),
            key(KeyCode::Esc),
        ]);
        let worktrees = entries(&["alpha", "beta", "gamma"]);
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let mut removed = Vec::new();
        let result = command.run(|name| {
            removed.push(name.to_owned());
            Ok(())
        })?;

        assert!(
            result.is_none(),
            "expected interactive session to exit without opening"
        );
        assert_eq!(removed, vec!["beta"]);

        Ok(())
    }

    #[test]
    fn cancelling_remove_keeps_worktree() -> Result<()> {
        let backend = TestBackend::new(40, 12);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![
            key(KeyCode::Tab),
            key(KeyCode::Down),
            key(KeyCode::Enter),
            key(KeyCode::Esc),
            key(KeyCode::Esc),
        ]);
        let worktrees = entries(&["alpha", "beta"]);
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let mut removed = Vec::new();
        let result = command.run(|name| {
            removed.push(name.to_owned());
            Ok(())
        })?;

        assert!(result.is_none());
        assert!(removed.is_empty());

        Ok(())
    }
}
