use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    SessionStore, Transcript, TranscriptItem,
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
                transcript: Transcript::new(vec![
                    TranscriptItem::User("Hello".into()),
                    TranscriptItem::Assistant("Working on it.".into()),
                ]),
            }],
        }
    }
}

#[test]
fn a_loaded_session_keeps_its_transcript_items_in_order() {
    let store = SessionStore::load(&mut OneSession);

    let session = store.session(SessionId(0)).unwrap();

    assert_eq!(
        session.transcript.items(),
        [
            TranscriptItem::User("Hello".into()),
            TranscriptItem::Assistant("Working on it.".into()),
        ]
    );
}
