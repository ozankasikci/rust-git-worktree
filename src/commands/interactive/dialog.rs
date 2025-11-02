use super::WorktreeEntry;

/// Calculates initial scroll position to center default branch
fn calculate_initial_scroll(
    selected_line: Option<usize>,
    total_lines: usize,
    visible_height: usize,
) -> usize {
    let selected = selected_line.unwrap_or(0);
    let ideal_center = selected.saturating_sub(visible_height / 2);
    let max_scroll = total_lines.saturating_sub(visible_height);
    ideal_center.min(max_scroll)
}

/// Represents different types of lines in the scrollable branch list
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LineType {
    /// A group header like "Branches" or "Worktrees"
    GroupHeader { title: String },

    /// A selectable branch option
    BranchOption {
        /// Index into base_groups
        group_idx: usize,
        /// Index into base_groups[group_idx].options
        option_idx: usize,
    },

    /// Empty spacing line between groups
    EmptyLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreateDialogFocus {
    Name,
    Base,
    Buttons,
}

#[derive(Clone, Debug)]
pub(crate) struct BaseOption {
    pub(crate) label: String,
    pub(crate) value: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct BaseOptionGroup {
    pub(crate) title: String,
    pub(crate) options: Vec<BaseOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct CreateDialog {
    pub(crate) name_input: String,
    pub(crate) focus: CreateDialogFocus,
    pub(crate) buttons_selected: usize,
    pub(crate) base_groups: Vec<BaseOptionGroup>,
    pub(crate) base_indices: Vec<(usize, usize)>,
    pub(crate) base_selected: usize,
    pub(crate) error: Option<String>,

    // Scroll state fields
    /// Flattened list of all renderable lines (headers + branches + spacing)
    pub(crate) flat_lines: Vec<LineType>,

    /// Index of the first visible line in the viewport
    pub(crate) scroll_offset: usize,

    /// Last known viewport height (for detecting resize)
    pub(crate) last_known_height: u16,

    /// Last known content height (viewport minus indicators)
    pub(crate) last_known_content_height: usize,
}

impl CreateDialog {
    pub(crate) fn new(
        branches: &[String],
        worktrees: &[WorktreeEntry],
        default_branch: Option<&str>,
    ) -> Self {
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

        // Build flat_lines from groups
        let mut flat_lines = Vec::new();
        for (group_idx, group) in groups.iter().enumerate() {
            flat_lines.push(LineType::GroupHeader {
                title: group.title.clone(),
            });

            for (option_idx, _) in group.options.iter().enumerate() {
                flat_lines.push(LineType::BranchOption {
                    group_idx,
                    option_idx,
                });
            }

            // Add spacing between groups (but not after last)
            if group_idx < groups.len() - 1 {
                flat_lines.push(LineType::EmptyLine);
            }
        }

        // Calculate initial scroll position to center default branch
        let selected_line =
            base_indices
                .get(base_selected)
                .and_then(|(target_group, target_option)| {
                    flat_lines.iter().position(|line| {
                        matches!(
                            line,
                            LineType::BranchOption { group_idx, option_idx }
                            if group_idx == target_group && option_idx == target_option
                        )
                    })
                });

        // Use reasonable default for initial visible height
        let initial_content_height = 6; // Conservative estimate
        let scroll_offset =
            calculate_initial_scroll(selected_line, flat_lines.len(), initial_content_height);
        let last_known_height = 0; // Will be set on first render

        Self {
            name_input: String::new(),
            focus: CreateDialogFocus::Name,
            buttons_selected: 0,
            base_groups: groups,
            base_indices,
            base_selected,
            error: None,
            flat_lines,
            scroll_offset,
            last_known_height,
            last_known_content_height: initial_content_height,
        }
    }

    pub(crate) fn base_option(&self) -> Option<&BaseOption> {
        self.base_indices
            .get(self.base_selected)
            .map(|(group_idx, option_idx)| &self.base_groups[*group_idx].options[*option_idx])
    }

    pub(crate) fn focus_next(&mut self) {
        self.focus = match self.focus {
            CreateDialogFocus::Name => CreateDialogFocus::Base,
            CreateDialogFocus::Base => CreateDialogFocus::Buttons,
            CreateDialogFocus::Buttons => CreateDialogFocus::Name,
        };
    }

    pub(crate) fn focus_prev(&mut self) {
        self.focus = match self.focus {
            CreateDialogFocus::Name => CreateDialogFocus::Buttons,
            CreateDialogFocus::Base => CreateDialogFocus::Name,
            CreateDialogFocus::Buttons => CreateDialogFocus::Base,
        };
    }

    pub(crate) fn move_base(&mut self, delta: isize) {
        if self.base_indices.is_empty() {
            return;
        }

        let len = self.base_indices.len() as isize;
        let current = self.base_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.base_selected = next as usize;

        // Update scroll position to keep selection visible
        // Use last known content height from rendering
        self.ensure_selected_visible(self.last_known_content_height);
    }

    pub(crate) fn find_selected_line(&self) -> Option<usize> {
        let (target_group, target_option) = self.base_indices.get(self.base_selected)?;

        self.flat_lines.iter().position(|line| {
            matches!(
                line,
                LineType::BranchOption { group_idx, option_idx }
                if group_idx == target_group && option_idx == target_option
            )
        })
    }

    pub(crate) fn ensure_selected_visible(&mut self, visible_height: usize) {
        const MARGIN: usize = 2;

        // If viewport is too small, just ensure selection is in range
        if visible_height == 0 {
            return;
        }

        let Some(selected_line) = self.find_selected_line() else {
            return;
        };

        let max_scroll = self.flat_lines.len().saturating_sub(visible_height);

        // Calculate safe lower bound (handle case where visible_height < MARGIN)
        let viewport_end = self.scroll_offset + visible_height;
        let safe_bottom = viewport_end.saturating_sub(MARGIN);

        // Scroll down if selection below viewport
        if selected_line >= safe_bottom {
            self.scroll_offset = (selected_line + MARGIN + 1)
                .saturating_sub(visible_height)
                .min(max_scroll);
        }

        // Scroll up if selection above viewport
        if selected_line < self.scroll_offset + MARGIN {
            self.scroll_offset = selected_line.saturating_sub(MARGIN);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CreateDialogView {
    pub(crate) name_input: String,
    pub(crate) focus: CreateDialogFocus,
    pub(crate) buttons_selected: usize,
    pub(crate) base_groups: Vec<BaseOptionGroup>,
    pub(crate) base_selected: usize,
    pub(crate) base_indices: Vec<(usize, usize)>,
    pub(crate) error: Option<String>,
    pub(crate) flat_lines: Vec<LineType>,
    pub(crate) scroll_offset: usize,
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
            flat_lines: dialog.flat_lines.clone(),
            scroll_offset: dialog.scroll_offset,
        }
    }
}

impl From<CreateDialog> for CreateDialogView {
    fn from(dialog: CreateDialog) -> Self {
        Self::from(&dialog)
    }
}

impl CreateDialogView {
    pub(crate) fn base_indices(&self) -> &[(usize, usize)] {
        &self.base_indices
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoveDialogFocus {
    Options,
    Buttons,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoveDialog {
    pub(crate) index: usize,
    pub(crate) focus: RemoveDialogFocus,
    pub(crate) options_selected: usize,
    pub(crate) buttons_selected: usize,
    pub(crate) remove_local_branch: bool,
}

impl RemoveDialog {
    const OPTION_COUNT: usize = 1;
    const BUTTON_COUNT: usize = 2;

    pub(crate) fn new(index: usize) -> Self {
        Self {
            index,
            focus: RemoveDialogFocus::Options,
            options_selected: 0,
            buttons_selected: 1,
            remove_local_branch: true,
        }
    }

    pub(crate) fn focus_next(&mut self) {
        self.focus = match self.focus {
            RemoveDialogFocus::Options => RemoveDialogFocus::Buttons,
            RemoveDialogFocus::Buttons => RemoveDialogFocus::Options,
        };
    }

    pub(crate) fn focus_prev(&mut self) {
        self.focus_next();
    }

    pub(crate) fn move_option(&mut self, delta: isize) {
        let len = Self::OPTION_COUNT as isize;
        let current = self.options_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.options_selected = next as usize;
    }

    pub(crate) fn move_button(&mut self, delta: isize) {
        let len = Self::BUTTON_COUNT as isize;
        let current = self.buttons_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.buttons_selected = next as usize;
    }

    pub(crate) fn toggle_selected_option(&mut self) {
        if self.options_selected == 0 {
            self.remove_local_branch = !self.remove_local_branch;
        }
    }

    pub(crate) fn remove_local_branch(&self) -> bool {
        self.remove_local_branch
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RemoveDialogView {
    pub(crate) focus: RemoveDialogFocus,
    pub(crate) options_selected: usize,
    pub(crate) buttons_selected: usize,
    pub(crate) remove_local_branch: bool,
}

impl From<&RemoveDialog> for RemoveDialogView {
    fn from(dialog: &RemoveDialog) -> Self {
        Self {
            focus: dialog.focus,
            options_selected: dialog.options_selected,
            buttons_selected: dialog.buttons_selected,
            remove_local_branch: dialog.remove_local_branch,
        }
    }
}

impl From<RemoveDialog> for RemoveDialogView {
    fn from(dialog: RemoveDialog) -> Self {
        Self::from(&dialog)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergeDialogFocus {
    Options,
    Buttons,
}

#[derive(Clone, Debug)]
pub(crate) struct MergeDialog {
    pub(crate) index: usize,
    pub(crate) focus: MergeDialogFocus,
    pub(crate) options_selected: usize,
    pub(crate) buttons_selected: usize,
    pub(crate) remove_local_branch: bool,
    pub(crate) remove_remote_branch: bool,
    pub(crate) remove_worktree: bool,
}

impl MergeDialog {
    const OPTION_COUNT: usize = 3;
    const BUTTON_COUNT: usize = 2;

    pub(crate) fn new(index: usize) -> Self {
        Self {
            index,
            focus: MergeDialogFocus::Options,
            options_selected: 0,
            buttons_selected: 1,
            remove_local_branch: true,
            remove_remote_branch: false,
            remove_worktree: false,
        }
    }

    pub(crate) fn focus_next(&mut self) {
        self.focus = match self.focus {
            MergeDialogFocus::Options => MergeDialogFocus::Buttons,
            MergeDialogFocus::Buttons => MergeDialogFocus::Options,
        };
    }

    pub(crate) fn focus_prev(&mut self) {
        self.focus_next();
    }

    pub(crate) fn move_option(&mut self, delta: isize) {
        let len = Self::OPTION_COUNT as isize;
        let current = self.options_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.options_selected = next as usize;
    }

    pub(crate) fn move_button(&mut self, delta: isize) {
        let len = Self::BUTTON_COUNT as isize;
        let current = self.buttons_selected as isize;
        let next = (current + delta).rem_euclid(len);
        self.buttons_selected = next as usize;
    }

    pub(crate) fn toggle_selected_option(&mut self) {
        match self.options_selected {
            0 => self.remove_local_branch = !self.remove_local_branch,
            1 => self.remove_remote_branch = !self.remove_remote_branch,
            2 => self.remove_worktree = !self.remove_worktree,
            _ => {}
        }
    }

    pub(crate) fn remove_local_branch(&self) -> bool {
        self.remove_local_branch
    }

    pub(crate) fn remove_remote_branch(&self) -> bool {
        self.remove_remote_branch
    }

    pub(crate) fn remove_worktree(&self) -> bool {
        self.remove_worktree
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MergeDialogView {
    pub(crate) focus: MergeDialogFocus,
    pub(crate) options_selected: usize,
    pub(crate) buttons_selected: usize,
    pub(crate) remove_local_branch: bool,
    pub(crate) remove_remote_branch: bool,
    pub(crate) remove_worktree: bool,
}

impl From<&MergeDialog> for MergeDialogView {
    fn from(dialog: &MergeDialog) -> Self {
        Self {
            focus: dialog.focus,
            options_selected: dialog.options_selected,
            buttons_selected: dialog.buttons_selected,
            remove_local_branch: dialog.remove_local_branch,
            remove_remote_branch: dialog.remove_remote_branch,
            remove_worktree: dialog.remove_worktree,
        }
    }
}

impl From<MergeDialog> for MergeDialogView {
    fn from(dialog: MergeDialog) -> Self {
        Self::from(&dialog)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum InfoDialogKind {
    Info,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) enum Dialog {
    Remove(RemoveDialog),
    Info {
        message: String,
        kind: InfoDialogKind,
    },
    Create(CreateDialog),
    Merge(MergeDialog),
}
