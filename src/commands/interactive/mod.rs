use std::io;

use color_eyre::eyre::WrapErr;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    Repo,
    commands::{
        cd::CdCommand,
        list::{find_worktrees, format_worktree},
    },
};

pub trait EventSource {
    fn next(&mut self) -> color_eyre::Result<Event>;
}

pub struct InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    terminal: Terminal<B>,
    events: E,
    worktrees: Vec<String>,
    selected: usize,
}

impl<B, E> InteractiveCommand<B, E>
where
    B: Backend,
    E: EventSource,
{
    pub fn new(terminal: Terminal<B>, events: E, worktrees: Vec<String>) -> Self {
        Self {
            terminal,
            events,
            worktrees,
            selected: 0,
        }
    }

    pub fn run(mut self) -> color_eyre::Result<Option<String>> {
        self.terminal
            .hide_cursor()
            .wrap_err("failed to hide cursor")?;

        let mut state = ListState::default();
        if self.worktrees.is_empty() {
            state.select(None);
        } else {
            if self.selected >= self.worktrees.len() {
                self.selected = 0;
            }
            state.select(Some(self.selected));
        }

        let selection = loop {
            self.terminal.draw(|frame| {
                let size = frame.size();
                if self.worktrees.is_empty() {
                    let block = Block::default().title("No worktrees").borders(Borders::ALL);
                    let paragraph =
                        Paragraph::new("No worktrees found. Press Esc to exit.").block(block);
                    frame.render_widget(paragraph, size);
                } else {
                    let items: Vec<ListItem> = self
                        .worktrees
                        .iter()
                        .map(|entry| ListItem::new(entry.as_str()))
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .title("Select a worktree")
                                .borders(Borders::ALL),
                        )
                        .highlight_symbol("▶ ")
                        .highlight_style(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        );
                    frame.render_stateful_widget(list, size, &mut state);
                }
            })?;

            let event = self.events.next()?;
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if self.worktrees.is_empty() {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => break None,
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => break None,
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.selected == 0 {
                                self.selected = self.worktrees.len() - 1;
                            } else {
                                self.selected -= 1;
                            }
                            state.select(Some(self.selected));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.selected = (self.selected + 1) % self.worktrees.len();
                            state.select(Some(self.selected));
                        }
                        KeyCode::Enter => {
                            let chosen = self.worktrees[self.selected].clone();
                            break Some(chosen);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        };

        self.terminal
            .show_cursor()
            .wrap_err("failed to show cursor")?;

        Ok(selection)
    }
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
        .iter()
        .map(|path| format_worktree(path))
        .collect::<Vec<_>>();

    enable_raw_mode().wrap_err("failed to enable raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen).wrap_err("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).wrap_err("failed to initialize terminal")?;
    let events = CrosstermEvents::default();

    let command = InteractiveCommand::new(terminal, events, worktrees);
    let selection_result = command.run();
    let cleanup_result = cleanup_terminal();
    if let Err(cleanup_err) = cleanup_result {
        let _ = selection_result;
        return Err(cleanup_err);
    }

    let selection = selection_result?;

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

    #[test]
    fn returns_first_worktree_when_enter_pressed_immediately() -> Result<()> {
        let backend = TestBackend::new(40, 10);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![key(KeyCode::Enter)]);
        let worktrees = vec!["alpha".into(), "beta".into()];
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let selection = command.run()?.expect("expected selection");
        assert_eq!(selection, "alpha");

        Ok(())
    }

    #[test]
    fn navigates_down_before_selecting() -> Result<()> {
        let backend = TestBackend::new(40, 10);
        let terminal = Terminal::new(backend)?;
        let events = StubEvents::new(vec![key(KeyCode::Down), key(KeyCode::Enter)]);
        let worktrees = vec!["alpha".into(), "beta".into(), "gamma".into()];
        let command = InteractiveCommand::new(terminal, events, worktrees);

        let selection = command.run()?.expect("expected selection");
        assert_eq!(selection, "beta");

        Ok(())
    }
}
