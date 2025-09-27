use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::{
    Action, Focus, StatusMessage,
    dialog::{CreateDialogFocus, CreateDialogView},
};

pub(crate) struct Snapshot {
    items: Vec<String>,
    detail: Option<DetailData>,
    focus: Focus,
    action_selected: usize,
    global_action_selected: usize,
    status: Option<StatusMessage>,
    dialog: Option<DialogView>,
    has_worktrees: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DetailData {
    pub(crate) lines: Vec<Line<'static>>,
}

#[derive(Clone, Debug)]
pub(crate) enum DialogView {
    ConfirmRemove { name: String },
    Info { message: String },
    Create(CreateDialogView),
}

impl Snapshot {
    pub(crate) fn new(
        items: Vec<String>,
        detail: Option<DetailData>,
        focus: Focus,
        action_selected: usize,
        global_action_selected: usize,
        status: Option<StatusMessage>,
        dialog: Option<DialogView>,
        has_worktrees: bool,
    ) -> Self {
        Self {
            items,
            detail,
            focus,
            action_selected,
            global_action_selected,
            status,
            dialog,
            has_worktrees,
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, state: &mut ListState) {
        let size = frame.size();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(size);

        let global_height = (super::GLOBAL_ACTIONS.len() as u16 + 2).max(3);

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(global_height), Constraint::Min(3)])
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

        let mut lines = if let Some(detail) = &self.detail {
            detail.lines.clone()
        } else {
            vec![Line::from("No worktree selected.")]
        };

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
                .title("Worktree Actions (Tab key)")
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
        let mut lines = Vec::new();
        for (idx, label) in super::GLOBAL_ACTIONS.iter().enumerate() {
            let mut style = Style::default();
            if self.focus == Focus::GlobalActions && self.global_action_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            lines.push(Line::from(vec![Span::styled(format!("[{label}]"), style)]));
        }

        let actions = Paragraph::new(lines).block(
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
