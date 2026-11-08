use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    SessionStore, Transcript,
};
use lodestar_core::sidebar::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, SidebarList};

/// Two Projects; the sessions arrive out of order so the sidebar has to sort them.
struct TwoProjects;

impl SessionSource for TwoProjects {
    fn initial_sessions(&mut self) -> InitialSessions {
        let project = |id, name: &str| Project {
            id: ProjectId(id),
            name: name.into(),
            path: format!("~/code/{name}").into(),
        };
        let session = |id, project, started_at| Session {
            id: SessionId(id),
            title: format!("Session {id}"),
            project: ProjectId(project),
            status: SessionStatus::Done,
            started_at,
            transcript: Transcript::default(),
        };
        InitialSessions {
            projects: vec![project(0, "lodestar"), project(1, "notes")],
            sessions: vec![
                session(10, 0, 100),
                session(20, 1, 500),
                session(11, 0, 300),
                session(21, 1, 200),
            ],
        }
    }
}

fn list() -> SidebarList {
    SidebarList::new(&SessionStore::load(&mut TwoProjects))
}

#[test]
fn rows_follow_project_order_with_newest_session_first_in_each() {
    let rows = list();

    assert_eq!(
        rows.sessions(),
        [SessionId(11), SessionId(10), SessionId(20), SessionId(21)]
    );
}

#[test]
fn focusing_the_sidebar_selects_the_focused_panes_session() {
    let rows = list();

    assert_eq!(rows.initial_selection(Some(SessionId(20))), 2);
}

#[test]
fn focusing_the_sidebar_from_an_empty_pane_selects_the_first_row() {
    let rows = list();

    assert_eq!(rows.initial_selection(None), 0);
    assert_eq!(rows.initial_selection(Some(SessionId(99))), 0);
}

#[test]
fn j_moves_down_and_stops_at_the_last_row() {
    let rows = list();

    assert_eq!(rows.step(0, 1), 1);
    assert_eq!(rows.step(3, 1), 3);
}

#[test]
fn k_moves_up_and_stops_at_the_first_row() {
    let rows = list();

    assert_eq!(rows.step(2, -1), 1);
    assert_eq!(rows.step(0, -1), 0);
}

#[test]
fn enter_opens_the_selected_row() {
    let rows = list();

    assert_eq!(rows.session_at(1), Some(SessionId(10)));
    assert_eq!(rows.session_at(4), None);
}

#[test]
fn an_empty_store_has_no_rows_and_no_selection_to_move() {
    let rows = SidebarList::new(&SessionStore::default());

    assert!(rows.sessions().is_empty());
    assert_eq!(rows.initial_selection(None), 0);
    assert_eq!(rows.step(0, 1), 0);
    assert_eq!(rows.session_at(0), None);
}

#[test]
fn dragging_the_edge_keeps_the_width_within_limits() {
    use lodestar_core::sidebar::clamp_width;

    assert_eq!(clamp_width(300.0), 300.0);
    assert_eq!(clamp_width(10.0), SIDEBAR_MIN_WIDTH);
    assert_eq!(clamp_width(5000.0), SIDEBAR_MAX_WIDTH);
}
