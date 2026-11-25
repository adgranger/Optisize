pub mod drives;
pub mod menu;
pub mod side;
pub mod status;
pub mod theme;
pub mod toolbar;
pub mod tree_view;
pub mod treemap;
pub mod types;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScanState {
    Idle,
    Scanning,
    Paused,
    Done,
    Stopped,
}

/// Commandes émises par l'UI, exécutées par `App`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    PickFolder,
    Scan,
    TogglePause,
    Stop,
    Refresh,
    Clean,
    Trash,
    Parent,
    Quit,
    OpenExplorer,
    CopyPath,
    ExpandAll,
    CollapseAll,
}

#[derive(Clone, Copy)]
pub struct Flags {
    pub state: ScanState,
    pub has_selection: bool,
}

impl Flags {
    pub fn busy(&self) -> bool {
        matches!(self.state, ScanState::Scanning | ScanState::Paused)
    }
}
