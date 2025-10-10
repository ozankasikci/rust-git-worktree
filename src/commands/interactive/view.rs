use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::command::ActionPanelState;
use super::{
    Action, Focus, StatusMessage,
    dialog::{
        CreateDialogFocus, CreateDialogView, LineType, MergeDialogFocus, MergeDialogView,
        RemoveDialogFocus, RemoveDialogView,
    },
};

pub(crate) struct Snapshot {
    items: Vec<String>,
    detail: Option<DetailData>,
    focus: Focus,
    action_panel: ActionPanelState,
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
    Remove {
        name: String,
        dialog: RemoveDialogView,
    },
    Info {
        message: String,
    },
    Create(CreateDialogView),
    Merge {
        name: String,
        dialog: MergeDialogView,
    },
}

impl Snapshot {
    pub(crate) fn new(
        items: Vec<String>,
        detail: Option<DetailData>,
        focus: Focus,
        action_panel: ActionPanelState,
        global_action_selected: usize,
        status: Option<StatusMessage>,
        dialog: Option<DialogView>,
        has_worktrees: bool,
    ) -> Self {
        Self {
            items,
            detail,
            focus,
            action_panel,
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
                DialogView::Remove { name, dialog } => {
                    self.render_remove(frame, size, name, dialog)
                }
                DialogView::Info { message } => self.render_info(frame, size, message),
                DialogView::Create(create) => self.render_create(frame, size, create),
                DialogView::Merge { name, dialog } => self.render_merge(frame, size, name, dialog),
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
        let list_height = (Action::ALL.len() as u16).saturating_add(2).max(3);
        let panel_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(list_height),
                Constraint::Length(3),
            ])
            .split(area);

        let detail_lines = if let Some(detail) = &self.detail {
            detail.lines.clone()
        } else {
            vec![Line::from("No worktree selected.")]
        };
        let detail_block = Paragraph::new(detail_lines)
            .block(Block::default().title("Details").borders(Borders::ALL));
        frame.render_widget(detail_block, panel_layout[0]);

        let mut items = Vec::new();
        for action in Action::ALL.iter() {
            let mut style = Style::default();
            if action.requires_selection() && !self.has_worktrees {
                style = style.add_modifier(Modifier::DIM);
            }
            items.push(ListItem::new(Line::from(Span::styled(
                format!("[{}]", action.label()),
                style,
            ))));
        }

        let is_actions_focused = self.focus == Focus::Actions;
        let mut list_state = ListState::default()
            .with_selected(Some(self.action_panel.selected_index))
            .with_offset(self.action_panel.scroll_offset);

        let highlight_style = if is_actions_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
        };
        let highlight_symbol = if is_actions_focused { "▶ " } else { "  " };

        let action_list = List::new(items)
            .block(
                Block::default()
                    .title("Worktree Actions (Tab key)")
                    .borders(Borders::ALL),
            )
            .highlight_symbol(highlight_symbol)
            .highlight_style(highlight_style);

        frame.render_stateful_widget(action_list, panel_layout[1], &mut list_state);

        let list_area = panel_layout[1];
        let visible_rows = usize::from(list_area.height.saturating_sub(2));
        let total_items = Action::ALL.len();
        let show_top_indicator = visible_rows > 0 && self.action_panel.scroll_offset > 0;
        let show_bottom_indicator =
            visible_rows > 0 && self.action_panel.scroll_offset + visible_rows < total_items;

        if list_area.width > 2 && list_area.height > 2 {
            let indicator_x = list_area
                .x
                .saturating_add(list_area.width.saturating_sub(2));

            if show_top_indicator {
                let top_area = Rect::new(indicator_x, list_area.y.saturating_add(1), 1, 1);
                frame.render_widget(
                    Paragraph::new("▲")
                        .alignment(Alignment::Right)
                        .style(Style::default().fg(Color::Gray)),
                    top_area,
                );
            }

            if show_bottom_indicator {
                let bottom_area = Rect::new(
                    indicator_x,
                    list_area
                        .y
                        .saturating_add(list_area.height.saturating_sub(2)),
                    1,
                    1,
                );
                frame.render_widget(
                    Paragraph::new("▼")
                        .alignment(Alignment::Right)
                        .style(Style::default().fg(Color::Gray)),
                    bottom_area,
                );
            }
        }

        let status_line = if let Some(status) = &self.status {
            Line::from(Span::styled(status.text.clone(), status.style()))
        } else {
            Line::from("Use Tab to focus actions. Esc exits.")
        };
        let status = Paragraph::new(vec![status_line])
            .block(Block::default().title("Status").borders(Borders::ALL));
        frame.render_widget(status, panel_layout[2]);
    }

    fn render_remove(&self, frame: &mut Frame, area: Rect, name: &str, dialog: &RemoveDialogView) {
        let popup_area = centered_rect(60, 45, area);
        frame.render_widget(Clear, popup_area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(5),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let header_lines = vec![
            Line::from(format!("Remove worktree `{name}`")),
            Line::from("Choose any additional cleanup before removing."),
        ];
        let header = Paragraph::new(header_lines).block(
            Block::default()
                .title("Confirm removal")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
        frame.render_widget(header, layout[0]);

        let options = [(dialog.remove_local_branch, "Remove local branch")];

        let mut option_lines = Vec::new();
        for (idx, (checked, label)) in options.iter().enumerate() {
            let checkbox = if *checked { "[x]" } else { "[ ]" };
            let mut style = Style::default();
            if dialog.focus == RemoveDialogFocus::Options && dialog.options_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            option_lines.push(Line::from(vec![
                Span::styled((*checkbox).to_string(), style),
                Span::raw(" "),
                Span::styled((*label).to_string(), style),
            ]));
        }
        option_lines.push(Line::from(""));
        option_lines.push(Line::from(Span::styled(
            "Space toggles options. Enter confirms.",
            Style::default().fg(Color::Gray),
        )));

        let options_block = Paragraph::new(option_lines).block(
            Block::default()
                .title("Cleanup options")
                .borders(Borders::ALL),
        );
        frame.render_widget(options_block, layout[1]);

        let buttons = ["Cancel", "Remove"];
        let mut button_spans = Vec::new();
        for (idx, label) in buttons.iter().enumerate() {
            if idx > 0 {
                button_spans.push(Span::raw("   "));
            }

            let mut style = Style::default();
            if dialog.focus == RemoveDialogFocus::Buttons && dialog.buttons_selected == idx {
                let color = if idx == 1 { Color::Red } else { Color::Cyan };
                style = style
                    .fg(color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            button_spans.push(Span::styled(format!("[ {label} ]"), style));
        }

        let buttons_block = Paragraph::new(Line::from(button_spans))
            .block(Block::default().title("Actions").borders(Borders::ALL));
        frame.render_widget(buttons_block, layout[2]);
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

        // Calculate available height for viewport (subtract 2 for borders)
        let available_height = layout[1].height.saturating_sub(2) as usize;

        let mut base_lines = Vec::new();

        // Reserve space for potential scroll indicators (up to 2 lines)
        // We need to calculate this before rendering to know how much content fits
        let will_show_top_indicator = dialog.scroll_offset > 0;
        let indicator_space = 2; // Always reserve space for both potential indicators
        let content_height = available_height.saturating_sub(indicator_space);
        let scroll_end = (dialog.scroll_offset + content_height).min(dialog.flat_lines.len());
        let will_show_bottom_indicator = scroll_end < dialog.flat_lines.len();

        // Add scroll-up indicator if scrolled down
        if will_show_top_indicator {
            base_lines.push(Line::from(Span::styled(
                "  ▲ more above",
                Style::default().fg(Color::DarkGray),
            )));
        }

        for line_type in &dialog.flat_lines[dialog.scroll_offset..scroll_end] {
            match line_type {
                LineType::GroupHeader { title } => {
                    base_lines.push(Line::from(vec![Span::styled(
                        title.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    )]));
                }
                LineType::BranchOption {
                    group_idx,
                    option_idx,
                } => {
                    let option = &dialog.base_groups[*group_idx].options[*option_idx];
                    let is_selected = dialog
                        .base_indices()
                        .iter()
                        .position(|&(g, o)| g == *group_idx && o == *option_idx)
                        .map_or(false, |idx| idx == dialog.base_selected);

                    let mut style = Style::default();
                    if is_selected {
                        style = style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
                    }

                    base_lines.push(Line::from(vec![Span::styled(option.label.clone(), style)]));
                }
                LineType::EmptyLine => {
                    base_lines.push(Line::from(""));
                }
            }
        }

        // Add scroll-down indicator if more content below
        if will_show_bottom_indicator {
            base_lines.push(Line::from(Span::styled(
                "  ▼ more below",
                Style::default().fg(Color::DarkGray),
            )));
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

    fn render_merge(&self, frame: &mut Frame, area: Rect, name: &str, dialog: &MergeDialogView) {
        let popup_area = centered_rect(70, 60, area);
        frame.render_widget(Clear, popup_area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(6),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let header_lines = vec![
            Line::from(format!("Merge PR for `{name}`")),
            Line::from("Choose the cleanup steps to run after merging."),
        ];
        let header = Paragraph::new(header_lines).block(
            Block::default()
                .title("Merge PR (GitHub)")
                .borders(Borders::ALL),
        );
        frame.render_widget(header, layout[0]);

        let options = [
            (dialog.remove_local_branch, "Remove local branch"),
            (dialog.remove_remote_branch, "Remove remote branch"),
            (dialog.remove_worktree, "Remove worktree"),
        ];

        let mut option_lines = Vec::new();
        for (idx, (checked, label)) in options.iter().enumerate() {
            let checkbox = if *checked { "[x]" } else { "[ ]" };
            let mut style = Style::default();
            if dialog.focus == MergeDialogFocus::Options && dialog.options_selected == idx {
                style = style
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            option_lines.push(Line::from(vec![
                Span::styled((*checkbox).to_string(), style),
                Span::raw(" "),
                Span::styled((*label).to_string(), style),
            ]));
        }
        option_lines.push(Line::from(""));
        option_lines.push(Line::from(Span::styled(
            "Space toggles options. Enter confirms.",
            Style::default().fg(Color::Gray),
        )));

        let options_block = Paragraph::new(option_lines).block(
            Block::default()
                .title("Cleanup options")
                .borders(Borders::ALL),
        );
        frame.render_widget(options_block, layout[1]);

        let buttons = ["Cancel", "Merge"];
        let mut button_spans = Vec::new();
        for (idx, label) in buttons.iter().enumerate() {
            if idx > 0 {
                button_spans.push(Span::raw("   "));
            }

            let mut style = Style::default();
            if dialog.focus == MergeDialogFocus::Buttons && dialog.buttons_selected == idx {
                let color = if idx == 1 { Color::Green } else { Color::Cyan };
                style = style
                    .fg(color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }

            button_spans.push(Span::styled(format!("[ {label} ]"), style));
        }

        let buttons_block = Paragraph::new(Line::from(button_spans))
            .block(Block::default().title("Actions").borders(Borders::ALL));
        frame.render_widget(buttons_block, layout[2]);
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
