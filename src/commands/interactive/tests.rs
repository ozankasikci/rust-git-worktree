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

// Scroll logic unit tests

#[test]
fn find_selected_line_locates_branch_in_flat_lines() {
    let branches = vec!["main".to_string(), "develop".to_string(), "feature".to_string()];
    let dialog = dialog::CreateDialog::new(&branches, &[], Some("develop"));

    let selected_line = dialog.find_selected_line();

    // Expected structure: GroupHeader "Branches", main (idx=1), develop (idx=2), feature (idx=3)
    assert_eq!(selected_line, Some(2));
}

#[test]
fn ensure_visible_scrolls_down_when_selection_below_viewport() {
    let branches: Vec<String> = (0..50).map(|i| format!("branch-{i}")).collect();
    let mut dialog = dialog::CreateDialog::new(&branches, &[], None);

    // Navigate to branch 40
    for _ in 0..40 {
        dialog.move_base(1);
    }

    // Simulate small viewport
    let visible_height = 10;
    dialog.ensure_selected_visible(visible_height);

    let selected_line = dialog.find_selected_line().unwrap();
    assert!(
        selected_line >= dialog.scroll_offset,
        "selection should be at or after scroll_offset"
    );
    assert!(
        selected_line < dialog.scroll_offset + visible_height,
        "selection should be before end of viewport"
    );
}

#[test]
fn ensure_visible_scrolls_up_when_selection_above_viewport() {
    let branches: Vec<String> = (0..50).map(|i| format!("branch-{i}")).collect();
    let mut dialog = dialog::CreateDialog::new(&branches, &[], None);

    // Navigate to end
    for _ in 0..45 {
        dialog.move_base(1);
    }
    dialog.ensure_selected_visible(10);

    // Now navigate back to beginning
    for _ in 0..45 {
        dialog.move_base(-1);
    }

    let selected_line = dialog.find_selected_line().unwrap();
    assert!(
        selected_line >= dialog.scroll_offset,
        "selection should be visible after scrolling up"
    );
}

#[test]
fn initial_scroll_centers_default_branch() {
    let branches: Vec<String> = (0..50).map(|i| format!("branch-{:02}", i)).collect();
    let dialog = dialog::CreateDialog::new(&branches, &[], Some("branch-25"));

    let selected_line = dialog.find_selected_line().unwrap();

    // With 10 visible lines (default in CreateDialog::new), scroll should position
    // selected line near center
    assert!(
        dialog.scroll_offset > 0,
        "should scroll down to show branch-25 in center"
    );

    // Verify selection is within reasonable center range
    let relative_pos = selected_line - dialog.scroll_offset;
    assert!(
        relative_pos >= 3 && relative_pos <= 7,
        "selected branch should be near center of viewport"
    );
}

#[test]
fn scroll_offset_never_exceeds_content_bounds() {
    let branches: Vec<String> = (0..10).map(|i| format!("branch-{i}")).collect();
    let mut dialog = dialog::CreateDialog::new(&branches, &[], None);

    // Simulate large viewport (larger than content)
    let visible_height = 100;
    dialog.ensure_selected_visible(visible_height);

    let max_offset = dialog.flat_lines.len().saturating_sub(visible_height);
    assert_eq!(dialog.scroll_offset, 0, "scroll should be 0 when viewport larger than content");
    assert!(dialog.scroll_offset <= max_offset);
}

#[test]
fn move_base_updates_scroll_position() {
    let branches: Vec<String> = (0..30).map(|i| format!("branch-{i}")).collect();
    let mut dialog = dialog::CreateDialog::new(&branches, &[], None);

    let initial_offset = dialog.scroll_offset;

    // Navigate down significantly
    for _ in 0..20 {
        dialog.move_base(1);
    }

    // Scroll should have moved to keep selection visible
    assert!(
        dialog.scroll_offset != initial_offset,
        "scroll offset should change after navigation"
    );

    let selected_line = dialog.find_selected_line().unwrap();
    // With default visible height of 10, selection should be visible
    assert!(selected_line >= dialog.scroll_offset);
}

#[test]
fn wrap_around_from_last_to_first_adjusts_scroll() {
    let branches: Vec<String> = (0..50).map(|i| format!("branch-{i}")).collect();
    let mut dialog = dialog::CreateDialog::new(&branches, &[], None);

    // Navigate to last branch
    for _ in 0..49 {
        dialog.move_base(1);
    }

    assert_eq!(dialog.base_selected, 49);

    // Wrap around to first
    dialog.move_base(1);

    assert_eq!(dialog.base_selected, 0);
    assert_eq!(dialog.scroll_offset, 0, "scroll should reset to top after wrap-around");
}

#[test]
fn scroll_with_multiple_groups() {
    let branches = vec!["main".to_string(), "develop".to_string()];
    let worktrees = vec![
        WorktreeEntry::new("wt1".into(), PathBuf::from("/tmp/wt1")),
        WorktreeEntry::new("wt2".into(), PathBuf::from("/tmp/wt2")),
    ];

    let dialog = dialog::CreateDialog::new(&branches, &worktrees, Some("main"));

    // Verify flat_lines includes both groups
    let has_branch_header = dialog.flat_lines.iter().any(|line| {
        matches!(line, dialog::LineType::GroupHeader { title } if title == "Branches")
    });
    let has_worktree_header = dialog.flat_lines.iter().any(|line| {
        matches!(line, dialog::LineType::GroupHeader { title } if title == "Worktrees")
    });

    assert!(has_branch_header, "should have Branches header");
    assert!(has_worktree_header, "should have Worktrees header");

    // Verify we can find the selected line
    assert!(dialog.find_selected_line().is_some());
}
