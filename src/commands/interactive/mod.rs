use std::{collections::BTreeSet, io, path::PathBuf};

use color_eyre::eyre::{self, WrapErr};
use crossterm::{
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use git2::BranchType;
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
        create::{CreateCommand, CreateOutcome},
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
    GlobalActions,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Worktrees => Focus::Actions,
            Focus::Actions => Focus::GlobalActions,
            Focus::GlobalActions => Focus::Worktrees,
        }
    }

    fn prev(self) -> Self {
        match self {
            Focus::Worktrees => Focus::GlobalActions,
            Focus::Actions => Focus::Worktrees,
            Focus::GlobalActions => Focus::Actions,
        }
    }
}

const GLOBAL_ACTIONS: [&str; 1] = ["Create"];

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
    Create(CreateDialog),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreateDialogFocus {
    Name,
    Base,
    Buttons,
}

#[derive(Clone, Debug)]
struct BaseOption {
    label: String,
    value: Option<String>,
}

#[derive(Clone, Debug)]
struct BaseOptionGroup {
    title: String,
    options: Vec<BaseOption>,
}

#[derive(Clone, Debug)]
struct CreateDialog {
    name_input: String,
    focus: CreateDialogFocus,
    buttons_selected: usize,
    base_groups: Vec<BaseOptionGroup>,
    base_indices: Vec<(usize, usize)>,
    base_selected: usize,
    error: Option<String>,
}

impl CreateDialog {
    fn new(branches: &[String], worktrees: &[WorktreeEntry], default_branch: Option<&str>) -> Self {
        let mut groups = Vec::new();

        if !branches.is_empty() {
            let options = branches
                .iter()
                .map(|branch| BaseOption {
                    label: format!("branch: {branch}"),
                    value: Some(branch.clone()),
                })
                .collect();
            groups.push(BaseOptionGroup {
                title: "Branches".into(),
                options,
            });
        }

        let mut worktree_options = worktrees
            .iter()
            .map(|entry| BaseOption {
                label: format!("worktree: {}", entry.name),
                value: Some(entry.name.clone()),
            })
            .collect::<Vec<_>>();
        worktree_options.sort_by(|a, b| a.label.cmp(&b.label));

        if !worktree_options.is_empty() {
            groups.push(BaseOptionGroup {
                title: "Worktrees".into(),
                options: worktree_options,
            });
        }

        if groups.is_empty() {
            groups.push(BaseOptionGroup {
                title: "General".into(),
                options: vec![BaseOption {
                    label: "HEAD".into(),
                    value: None,
                }],
            });
        }

        let mut base_indices = Vec::new();
        for (group_idx, group) in groups.iter().enumerate() {
            for (option_idx, _) in group.options.iter().enumerate() {
                base_indices.push((group_idx, option_idx));
            }
        }

        let mut base_selected = 0;
        if let Some(default) = default_branch {
            if let Some((idx, _)) =
                base_indices
                    .iter()
                    .enumerate()
                    .find(|(_, (group_idx, option_idx))| {
                        groups[*group_idx].options[*option_idx]
                            .value
                            .as_deref()
                            .map_or(false, |value| value == default)
                    })
            {
                base_selected = idx;
            }
        }

        if base_indices.is_empty() {
            base_indices.push((0, 0));
            base_selected = 0;
        }

        Self {
            name_input: String::new(),
            focus: CreateDialogFocus::Name,
            buttons_selected: 0,
            base_groups: groups,
            base_indices,
            base_selected,
            error: None,
        }
    }

    fn base_option(&self) -> Option<&BaseOption> {
        self.base_indices
            .get(self.base_selected)
            .map(|(group_idx, option_idx)| &self.base_groups[*group_idx].options[*option_idx])
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            CreateDialogFocus::Name => CreateDialogFocus::Base,
            CreateDialogFocus::Base => CreateDialogFocus::Buttons,
            CreateDialogFocus::Buttons => CreateDialogFocus::Name,
        };
    }

    fn focus_prev(&mut self) {
        self.focus = match self.focus {
            CreateDialogFocus::Name => CreateDialogFocus::Buttons,
            CreateDialogFocus::Base => CreateDialogFocus::Name,
            CreateDialogFocus::Buttons => CreateDialogFocus::Base,
        };
    }

    fn move_base(&mut self, delta: isize) {
        if self.base_indices.is_empty() {
            return;
        }

        let len = self.base_indices.len() as isize;
        let current = self.base_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.base_selected = next as usize;
    }
}

#[derive(Clone, Debug)]
struct CreateDialogView {
    name_input: String,
    focus: CreateDialogFocus,
    buttons_selected: usize,
    base_groups: Vec<BaseOptionGroup>,
    base_selected: usize,
    base_indices: Vec<(usize, usize)>,
    error: Option<String>,
}

impl From<&CreateDialog> for CreateDialogView {
    fn from(dialog: &CreateDialog) -> Self {
        Self {
            name_input: dialog.name_input.clone(),
            focus: dialog.focus,
            buttons_selected: dialog.buttons_selected,
            base_groups: dialog.base_groups.clone(),
            base_selected: dialog.base_selected,
            base_indices: dialog.base_indices.clone(),
            error: dialog.error.clone(),
        }
    }
}

impl From<CreateDialog> for CreateDialogView {
    fn from(dialog: CreateDialog) -> Self {
        Self::from(&dialog)
    }
}

impl CreateDialogView {
    fn base_indices(&self) -> &[(usize, usize)] {
        &self.base_indices
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
    Create(CreateDialogView),
}

#[derive(Clone, Debug)]
struct SnapshotData {
    items: Vec<String>,
    detail: Option<DetailData>,
    focus: Focus,
    action_selected: usize,
    global_action_selected: usize,
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
            Some(Dialog::Create(dialog)) => Some(DialogView::Create(dialog.into())),
            None => None,
        };

        Self {
            items,
            detail,
            focus: command.focus,
            action_selected: command.action_selected,
            global_action_selected: command.global_action_selected,
            status: command.status.clone(),
            dialog,
            has_worktrees: !command.worktrees.is_empty(),
        }
    }

    fn render(&self, frame: &mut Frame, state: &mut ListState) {
        let size = frame.size();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(size);

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(columns[0]);

        self.render_global_actions(frame, left[0]);
        self.render_list(frame, left[1], state);
        self.render_details(frame, columns[1]);

        if let Some(dialog) = &self.dialog {
            match dialog {
                DialogView::ConfirmRemove { name } => self.render_confirmation(frame, size, name),
                DialogView::Info { message } => self.render_info(frame, size, message),
                DialogView::Create(create) => self.render_create(frame, size, create),
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

        let actions = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .title("Worktree Actions")
                .borders(Borders::ALL),
        );
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

    fn render_global_actions(&self, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        for (idx, label) in GLOBAL_ACTIONS.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw("  "));
            }

            let mut style = Style::default();
            if self.focus == Focus::GlobalActions && self.global_action_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            spans.push(Span::styled(format!("[{label}]"), style));
        }

        let actions = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .title("Global Actions")
                .borders(Borders::ALL),
        );
        frame.render_widget(actions, area);
    }

    fn render_create(&self, frame: &mut Frame, area: Rect, dialog: &CreateDialogView) {
        let popup_area = centered_rect(70, 70, area);
        frame.render_widget(Clear, popup_area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let mut name_block = Block::default()
            .title("Worktree Name")
            .borders(Borders::ALL);
        if dialog.focus == CreateDialogFocus::Name {
            name_block = name_block.border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
        }

        let name_value = if dialog.name_input.is_empty() {
            Span::styled("<enter name>", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(dialog.name_input.clone())
        };
        let name_line = Line::from(vec![
            Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
            name_value,
        ]);
        frame.render_widget(Paragraph::new(name_line).block(name_block), layout[0]);

        let mut base_lines = Vec::new();
        for (group_idx, group) in dialog.base_groups.iter().enumerate() {
            base_lines.push(Line::from(vec![Span::styled(
                group.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )]));

            for (option_idx, option) in group.options.iter().enumerate() {
                let selected = dialog
                    .base_indices()
                    .iter()
                    .position(|&(g, o)| g == group_idx && o == option_idx)
                    .map_or(false, |idx| idx == dialog.base_selected);

                let mut style = Style::default();
                if selected {
                    style = style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
                }

                base_lines.push(Line::from(vec![Span::styled(option.label.clone(), style)]));
            }

            base_lines.push(Line::from(""));
        }

        let mut base_block = Block::default()
            .title("Base Reference")
            .borders(Borders::ALL);
        if dialog.focus == CreateDialogFocus::Base {
            base_block = base_block.border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
        }
        frame.render_widget(Paragraph::new(base_lines).block(base_block), layout[1]);

        let mut footer_lines = Vec::new();
        if let Some(error) = &dialog.error {
            footer_lines.push(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            footer_lines.push(Line::from(""));
        }

        let mut button_spans = Vec::new();
        for (idx, label) in ["Create", "Cancel"].iter().enumerate() {
            if idx > 0 {
                button_spans.push(Span::raw("  "));
            }

            let mut style = Style::default();
            if dialog.focus == CreateDialogFocus::Buttons && dialog.buttons_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            button_spans.push(Span::styled(format!("[{label}]"), style));
        }
        footer_lines.push(Line::from(button_spans));

        let footer = Paragraph::new(footer_lines)
            .block(Block::default().title("Actions").borders(Borders::ALL));
        frame.render_widget(footer, layout[2]);
    }

    fn list_highlight_style(&self) -> Style {
        match self.focus {
            Focus::Worktrees => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Focus::Actions | Focus::GlobalActions => Style::default().add_modifier(Modifier::DIM),
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
    worktrees_dir: PathBuf,
    worktrees: Vec<WorktreeEntry>,
    selected: Option<usize>,
    focus: Focus,
    action_selected: usize,
    global_action_selected: usize,
    branches: Vec<String>,
    default_branch: Option<String>,
    status: Option<StatusMessage>,
    dialog: Option<Dialog>,
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

    pub fn run<F, G>(
        mut self,
        mut on_remove: F,
        mut on_create: G,
    ) -> color_eyre::Result<Option<String>>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
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

    fn event_loop<F, G>(
        &mut self,
        on_remove: &mut F,
        on_create: &mut G,
    ) -> color_eyre::Result<Option<String>>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
    {
        let mut state = ListState::default();
        self.sync_selection(&mut state);

        loop {
            let snapshot = SnapshotData::capture(self);
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
    ) -> color_eyre::Result<LoopControl>
    where
        F: FnMut(&str) -> color_eyre::Result<()>,
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
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
            Focus::GlobalActions => self.execute_global_action(),
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

    fn execute_global_action(&mut self) -> color_eyre::Result<LoopControl> {
        match self.global_action_selected {
            0 => {
                self.open_create_dialog();
                Ok(LoopControl::Continue)
            }
            _ => Ok(LoopControl::Continue),
        }
    }

    fn open_create_dialog(&mut self) {
        let dialog = CreateDialog::new(
            &self.branches,
            &self.worktrees,
            self.default_branch.as_deref(),
        );
        self.dialog = Some(Dialog::Create(dialog));
        self.status = None;
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

    fn handle_create_key<G>(
        &mut self,
        key: KeyEvent,
        state: &mut ListState,
        on_create: &mut G,
    ) -> color_eyre::Result<()>
    where
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
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
                // The outer scope will perform the actual close and status update.
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
    ) -> color_eyre::Result<Option<(String, String)>>
    where
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
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
    ) -> color_eyre::Result<Option<(String, String)>>
    where
        G: FnMut(&str, Option<&str>) -> color_eyre::Result<()>,
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
        let len = GLOBAL_ACTIONS.len() as isize;
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

    let (branches, default_branch) = load_branches(repo)?;

    enable_raw_mode().wrap_err("failed to enable raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen).wrap_err("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).wrap_err("failed to initialize terminal")?;
    let events = CrosstermEvents::default();

    let command = InteractiveCommand::new(
        terminal,
        events,
        worktrees_dir.clone(),
        worktrees,
        branches,
        default_branch,
    );
    let result = command.run(
        |name| {
            let command = RemoveCommand::new(name.to_owned(), false).with_quiet(true);
            command.execute(repo)
        },
        |name, base| {
            let command = CreateCommand::new(name.to_owned(), base.map(|b| b.to_owned()));
            match command.create_without_enter(repo, true)? {
                CreateOutcome::Created => Ok(()),
                CreateOutcome::AlreadyExists => {
                    Err(eyre::eyre!("Worktree `{}` already exists.", name))
                }
            }
        },
    );
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

fn load_branches(repo: &Repo) -> color_eyre::Result<(Vec<String>, Option<String>)> {
    let git_repo = repo.git();
    let mut set = BTreeSet::new();
    let mut default_branch = None;

    if let Ok(head) = git_repo.head() {
        if head.is_branch() {
            if let Some(name) = head.shorthand() {
                let branch = name.to_string();
                set.insert(branch.clone());
                default_branch = Some(branch);
            }
        }
    }

    let mut iter = git_repo.branches(Some(BranchType::Local))?;
    while let Some(branch_result) = iter.next() {
        let (branch, _) = branch_result?;
        if let Some(name) = branch.name()? {
            if !name.is_empty() {
                set.insert(name.to_string());
            }
        }
    }

    let branches: Vec<String> = set.into_iter().collect();
    let default_branch = default_branch.and_then(|branch| {
        if branches.iter().any(|candidate| candidate == &branch) {
            Some(branch)
        } else {
            None
        }
    });

    Ok((branches, default_branch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, path::PathBuf};

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
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let selection = command
            .run(|_| Ok(()), |_, _| panic!("create should not be called"))?
            .expect("expected selection");
        assert_eq!(selection, "alpha");

        Ok(())
    }

    #[test]
    fn navigates_down_before_selecting() -> Result<()> {
        let backend = TestBackend::new(40, 10);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![key(KeyCode::Down), key(KeyCode::Enter)]);
        let worktrees = entries(&["alpha", "beta", "gamma"]);
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let selection = command
            .run(|_| Ok(()), |_, _| panic!("create should not be called"))?
            .expect("expected selection");
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
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let mut removed = Vec::new();
        let result = command.run(
            |name| {
                removed.push(name.to_owned());
                Ok(())
            },
            |_, _| panic!("create should not be called"),
        )?;

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
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let mut removed = Vec::new();
        let result = command.run(
            |name| {
                removed.push(name.to_owned());
                Ok(())
            },
            |_, _| panic!("create should not be called"),
        )?;

        assert!(result.is_none());
        assert!(removed.is_empty());

        Ok(())
    }

    #[test]
    fn create_action_adds_new_worktree() -> Result<()> {
        let backend = TestBackend::new(60, 18);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![
            key(KeyCode::Tab),
            key(KeyCode::Tab),
            key(KeyCode::Enter),
            char_key('n'),
            char_key('e'),
            char_key('w'),
            key(KeyCode::Tab),
            key(KeyCode::Tab),
            key(KeyCode::Enter),
            key(KeyCode::Enter),
        ]);

        let worktrees = entries(&["alpha"]);
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let mut created = Vec::new();
        let result = command.run(
            |_| Ok(()),
            |name, base| {
                created.push((name.to_string(), base.map(|b| b.to_string())));
                Ok(())
            },
        )?;

        assert_eq!(result, Some(String::from("new")));
        assert_eq!(
            created,
            vec![(String::from("new"), Some(String::from("main")))]
        );

        Ok(())
    }

    #[test]
    fn cancelling_create_leaves_state_unchanged() -> Result<()> {
        let backend = TestBackend::new(60, 18);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![
            key(KeyCode::Tab),
            key(KeyCode::Tab),
            key(KeyCode::Enter),
            key(KeyCode::Esc),
            key(KeyCode::Esc),
        ]);

        let worktrees = entries(&["alpha"]);
        let command = InteractiveCommand::new(
            terminal,
            events,
            PathBuf::from("/tmp/worktrees"),
            worktrees,
            vec![String::from("main")],
            Some(String::from("main")),
        );

        let result = command.run(|_| Ok(()), |_, _| panic!("create should not be called"))?;

        assert!(result.is_none());

        Ok(())
    }
}
