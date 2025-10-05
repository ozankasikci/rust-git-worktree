use super::*;
use std::{collections::VecDeque, path::PathBuf};

use color_eyre::{Result, eyre};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use crate::commands::rm::{LocalBranchStatus, RemoveOutcome};

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
        .run(
            |_, _| {
                Ok(RemoveOutcome {
                    local_branch: None,
                    repositioned: false,
                })
            },
            |_, _| panic!("create should not be called"),
        )?
        .expect("expected selection");
    assert_eq!(selection, Selection::Worktree(String::from("alpha")));

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
        .run(
            |_, _| {
                Ok(RemoveOutcome {
                    local_branch: None,
                    repositioned: false,
                })
            },
            |_, _| panic!("create should not be called"),
        )?
        .expect("expected selection");
    assert_eq!(selection, Selection::Worktree(String::from("beta")));

    Ok(())
}

#[test]
fn selecting_pr_github_action_exits_with_pr_variant() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Enter),
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
        |name, _remove_branch| {
            removed.push(name.to_owned());
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    assert!(removed.is_empty(), "remove should not be triggered");
    assert_eq!(result, Some(Selection::PrGithub(String::from("alpha"))));

    Ok(())
}

#[test]
fn selecting_merge_action_collects_cleanup_choices() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Enter),
        key(KeyCode::Down),
        char_key(' '),
        key(KeyCode::Down),
        char_key(' '),
        key(KeyCode::Tab),
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

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    match result {
        Some(Selection::MergePrGithub {
            name,
            remove_local_branch,
            remove_remote_branch,
            remove_worktree,
        }) => {
            assert_eq!(name, "alpha");
            assert!(remove_local_branch);
            assert!(remove_remote_branch);
            assert!(remove_worktree);
        }
        other => panic!("unexpected selection: {other:?}"),
    }

    Ok(())
}

#[test]
fn merge_dialog_allows_disabling_local_branch_removal() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Enter),
        char_key(' '),
        key(KeyCode::Tab),
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

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    match result {
        Some(Selection::MergePrGithub {
            name,
            remove_local_branch,
            remove_remote_branch,
            remove_worktree,
        }) => {
            assert_eq!(name, "alpha");
            assert!(!remove_local_branch);
            assert!(!remove_remote_branch);
            assert!(!remove_worktree);
        }
        other => panic!("unexpected selection: {other:?}"),
    }

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
        |name, remove_local_branch| {
            removed.push((name.to_owned(), remove_local_branch));
            Ok(RemoveOutcome {
                local_branch: remove_local_branch.then_some(LocalBranchStatus::Deleted),
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    assert!(
        result.is_none(),
        "expected interactive session to exit without opening"
    );
    assert_eq!(removed, vec![(String::from("beta"), true)]);

    Ok(())
}

#[test]
fn remove_dialog_allows_disabling_local_branch_removal() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Down),
        key(KeyCode::Enter),
        char_key(' '),
        key(KeyCode::Tab),
        key(KeyCode::Enter),
        key(KeyCode::Enter),
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

    let mut removed = Vec::new();
    let result = command.run(
        |name, remove_local_branch| {
            removed.push((name.to_owned(), remove_local_branch));
            Ok(RemoveOutcome {
                local_branch: remove_local_branch.then_some(LocalBranchStatus::Deleted),
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    assert!(result.is_none());
    assert_eq!(removed, vec![(String::from("alpha"), false)]);

    Ok(())
}

#[test]
fn removing_current_worktree_requests_root_exit() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Down),
        key(KeyCode::Enter),
        char_key('y'),
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

    let mut removed = Vec::new();
    let result = command.run(
        |name, remove_local_branch| {
            removed.push((name.to_owned(), remove_local_branch));
            Ok(RemoveOutcome {
                local_branch: remove_local_branch.then_some(LocalBranchStatus::Deleted),
                repositioned: true,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    assert_eq!(removed, vec![(String::from("alpha"), true)]);
    assert_eq!(result, Some(Selection::RepoRoot));

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
        |name, remove_local_branch| {
            removed.push((name.to_owned(), remove_local_branch));
            Ok(RemoveOutcome {
                local_branch: remove_local_branch.then_some(LocalBranchStatus::Deleted),
                repositioned: false,
            })
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
        key(KeyCode::Up),
        key(KeyCode::Up),
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
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |name, base| {
            created.push((name.to_string(), base.map(|b| b.to_string())));
            Ok(())
        },
    )?;

    assert_eq!(result, Some(Selection::Worktree(String::from("new"))));
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
        key(KeyCode::Up),
        key(KeyCode::Up),
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

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| panic!("create should not be called"),
    )?;

    assert!(result.is_none());

    Ok(())
}

#[test]
fn cd_to_root_global_action_exits() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![key(KeyCode::Up), key(KeyCode::Enter)]);

    let worktrees = entries(&["alpha"]);
    let command = InteractiveCommand::new(
        terminal,
        events,
        PathBuf::from("/tmp/worktrees"),
        worktrees,
        vec![String::from("main")],
        Some(String::from("main")),
    );

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| Ok(()),
    )?;

    assert_eq!(result, Some(Selection::RepoRoot));

    Ok(())
}

#[test]
fn up_from_top_moves_to_global_actions() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![key(KeyCode::Up), key(KeyCode::Enter)]);

    let worktrees = entries(&["alpha"]);
    let command = InteractiveCommand::new(
        terminal,
        events,
        PathBuf::from("/tmp/worktrees"),
        worktrees,
        vec![String::from("main")],
        Some(String::from("main")),
    );

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| Ok(()),
    )?;

    assert_eq!(result, Some(Selection::RepoRoot));

    Ok(())
}

#[test]
fn up_from_top_after_tabbing_picks_last_global_action() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Tab),
        key(KeyCode::Tab),
        key(KeyCode::Up),
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

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| Ok(()),
    )?;

    assert_eq!(result, Some(Selection::RepoRoot));

    Ok(())
}

#[test]
fn up_with_no_worktrees_moves_to_global_actions() -> Result<()> {
    let backend = TestBackend::new(40, 12);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![key(KeyCode::Up), key(KeyCode::Enter)]);

    let worktrees = Vec::new();
    let command = InteractiveCommand::new(
        terminal,
        events,
        PathBuf::from("/tmp/worktrees"),
        worktrees,
        vec![String::from("main")],
        Some(String::from("main")),
    );

    let result = command.run(
        |_, _| {
            Ok(RemoveOutcome {
                local_branch: None,
                repositioned: false,
            })
        },
        |_, _| Ok(()),
    )?;

    assert_eq!(result, Some(Selection::RepoRoot));

    Ok(())
}

#[test]
fn down_with_no_worktrees_opens_create_dialog() -> Result<()> {
    let backend = TestBackend::new(60, 18);
    let terminal = Terminal::new(backend)?;
    let events = StubEvents::new(vec![
        key(KeyCode::Down),
        key(KeyCode::Enter),
        char_key('n'),
        char_key('e'),
        char_key('w'),
        key(KeyCode::Tab),
        key(KeyCode::Tab),
        key(KeyCode::Enter),
        key(KeyCode::Enter),
    ]);

    let worktrees = Vec::new();
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
        |_, _| panic!("remove should not be called"),
        |name, base| {
            created.push((name.to_string(), base.map(|b| b.to_string())));
            Ok(())
        },
    )?;

    assert_eq!(result, Some(Selection::Worktree(String::from("new"))));
    assert_eq!(
        created,
        vec![(String::from("new"), Some(String::from("main")))]
    );

    Ok(())
}
