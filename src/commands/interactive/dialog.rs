use super::WorktreeEntry;

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
    pub(crate) fn base_indices(&self) -> &[(usize, usize)] {
        &self.base_indices
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Dialog {
    ConfirmRemove { index: usize },
    Info { message: String },
    Create(CreateDialog),
}
