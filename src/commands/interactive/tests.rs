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
    fn next(&mut self) -> Result<Event> {
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

#[test]
fn cd_to_root_global_action_exits() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Tab),
        key(KeyCode::Right),
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

    let result = command.run(|_| Ok(()), |_, _| Ok(()))?;

    assert_eq!(result, Some(String::from(super::REPO_ROOT_SELECTION)));

    Ok(())
}

#[test]
fn up_from_top_moves_to_global_actions() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Up),
        key(KeyCode::Right),
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

    let result = command.run(|_| Ok(()), |_, _| Ok(()))?;

    assert_eq!(result, Some(String::from(super::REPO_ROOT_SELECTION)));

    Ok(())
}
