use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    Overview,
    Explorer,
    Board,
    Workspace,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub view: AppView,
    pub selected: Option<Uuid>,
    pub search: String,
    pub status_filter: Option<String>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            view: AppView::Overview,
            selected: None,
            search: String::new(),
            status_filter: None,
        }
    }
}
