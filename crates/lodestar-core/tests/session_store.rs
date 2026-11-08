use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    SessionStore, Transcript,
};

struct OneSession;

impl SessionSource for OneSession {
    fn initial_sessions(&mut self) -> InitialSessions {
        InitialSessions {
            projects: vec![Project {
                id: ProjectId(0),
                name: "lodestar".into(),
                path: "~/code/lodestar".into(),
            }],
            sessions: vec![Session {
                id: SessionId(0),
                title: "Pane tree".into(),
                project: ProjectId(0),
                status: SessionStatus::Running,
                started_at: 1,
                transcript: Transcript::new(vec!["Hello".into(), "Working on it.".into()]),
            }],
        }
    }
}

#[test]
fn a_loaded_session_keeps_its_transcript_items_in_order() {
    let store = SessionStore::load(&mut OneSession);

    let session = store.session(SessionId(0)).unwrap();

    assert_eq!(session.transcript.items(), ["Hello", "Working on it."]);
}

mod new_sessions {
    use lodestar_core::session::{
        InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
        SessionStore, Transcript,
    };

    /// Two Projects; lodestar has two sessions, dotfiles has none.
    struct Fixtures;

    fn session(id: u64, title: &str, project: u64, started_at: u64) -> Session {
        Session {
            id: SessionId(id),
            title: title.into(),
            project: ProjectId(project),
            status: SessionStatus::Done,
            started_at,
            transcript: Transcript::new(vec!["Hello".into()]),
        }
    }

    impl SessionSource for Fixtures {
        fn initial_sessions(&mut self) -> InitialSessions {
            InitialSessions {
                projects: vec![
                    Project {
                        id: ProjectId(0),
                        name: "lodestar".into(),
                        path: "/home/me/code/lodestar".into(),
                    },
                    Project {
                        id: ProjectId(1),
                        name: "dotfiles".into(),
                        path: "/home/me/dotfiles".into(),
                    },
                ],
                sessions: vec![
                    session(0, "Pane tree", 0, 100),
                    session(1, "CI on Ubuntu", 0, 50),
                ],
            }
        }
    }

    #[test]
    fn a_new_session_waits_for_input_with_an_empty_transcript() {
        let mut store = SessionStore::load(&mut Fixtures);

        let id = store.create_session(ProjectId(0));

        let created = store.session(id).unwrap();
        assert_eq!(created.project, ProjectId(0));
        assert_eq!(created.status, SessionStatus::WaitingForInput);
        assert!(created.transcript.items().is_empty());
    }
}
