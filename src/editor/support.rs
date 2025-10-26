use std::ffi::OsStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedEditor {
    Vim,
    Cursor,
    WebStorm,
    Rider,
}

impl SupportedEditor {
    pub const ALL: [SupportedEditor; 4] = [
        SupportedEditor::Vim,
        SupportedEditor::Cursor,
        SupportedEditor::WebStorm,
        SupportedEditor::Rider,
    ];

    pub fn command(self) -> &'static str {
        match self {
            SupportedEditor::Vim => "vim",
            SupportedEditor::Cursor => "cursor",
            SupportedEditor::WebStorm => "webstorm",
            SupportedEditor::Rider => "rider",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SupportedEditor::Vim => "Vim",
            SupportedEditor::Cursor => "Cursor",
            SupportedEditor::WebStorm => "WebStorm",
            SupportedEditor::Rider => "Rider",
        }
    }

    pub fn matches_command(self, command: &OsStr) -> bool {
        command == Self::command(self)
    }
}

pub fn supported_editor_commands() -> impl Iterator<Item = (&'static str, &'static str)> {
    SupportedEditor::ALL
        .iter()
        .map(|editor| (editor.command(), editor.label()))
}
