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

mod new_sessions {
    use lodestar_core::session::{
        InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
        SessionStore, Transcript, TranscriptItem,
    };

    /// Two Projects; lodestar has two sessions, dotfiles has none.
    pub struct Fixtures;

    fn session(id: u64, title: &str, project: u64, started_at: u64) -> Session {
        Session {
            id: SessionId(id),
            title: title.into(),
            project: ProjectId(project),
            status: SessionStatus::Done,
            started_at,
            transcript: Transcript::new(vec![TranscriptItem::User("Hello".into())]),
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

mod naming {
    use super::new_sessions::Fixtures;
    use lodestar_core::session::{ProjectId, SessionStore};

    #[test]
    fn a_new_session_is_titled_after_its_project_and_counted_within_it() {
        let mut store = SessionStore::load(&mut Fixtures);

        let third_in_lodestar = store.create_session(ProjectId(0));
        let first_in_dotfiles = store.create_session(ProjectId(1));

        assert_eq!(
            store.session(third_in_lodestar).unwrap().title,
            "lodestar #3"
        );
        assert_eq!(
            store.session(first_in_dotfiles).unwrap().title,
            "dotfiles #1"
        );
    }
}

mod ordering {
    use super::new_sessions::Fixtures;
    use lodestar_core::session::{ProjectId, SessionId, SessionStore};

    #[test]
    fn a_new_session_is_the_newest_of_all() {
        let mut store = SessionStore::load(&mut Fixtures);

        let created = store.create_session(ProjectId(1));

        let ids: Vec<SessionId> = store
            .sessions_newest_first()
            .iter()
            .map(|session| session.id)
            .collect();
        assert_eq!(ids, vec![created, SessionId(0), SessionId(1)]);
        assert_eq!(store.sessions_in(ProjectId(1))[0].id, created);
    }
}

mod recent_folders {
    use std::path::PathBuf;

    use super::new_sessions::Fixtures;
    use lodestar_core::session::{ProjectId, SessionStore};

    #[test]
    fn recent_folders_start_as_the_fixture_projects_paths() {
        let store = SessionStore::load(&mut Fixtures);

        assert_eq!(
            store.recent_folders(),
            [
                PathBuf::from("/home/me/code/lodestar"),
                PathBuf::from("/home/me/dotfiles")
            ]
        );
    }

    #[test]
    fn a_browsed_folder_joins_the_front_of_the_recent_list_once() {
        let mut store = SessionStore::load(&mut Fixtures);

        store.add_recent_folder("/home/me/code/pixel-forge".into());
        store.add_recent_folder("/home/me/dotfiles".into());
        store.add_recent_folder("/home/me/code/pixel-forge".into());

        assert_eq!(
            store.recent_folders(),
            [
                PathBuf::from("/home/me/code/pixel-forge"),
                PathBuf::from("/home/me/dotfiles"),
                PathBuf::from("/home/me/code/lodestar"),
            ]
        );
    }

    #[test]
    fn a_browsed_folder_becomes_a_project_named_after_its_last_component() {
        let mut store = SessionStore::load(&mut Fixtures);

        let project = store.find_or_create_project("/home/me/code/pixel-forge".into());

        let created = store.project(project).unwrap();
        assert_eq!(created.name, "pixel-forge");
        assert_eq!(created.path, PathBuf::from("/home/me/code/pixel-forge"));
        assert_eq!(store.projects().len(), 3);
    }

    #[test]
    fn browsing_to_an_existing_projects_folder_finds_that_project() {
        let mut store = SessionStore::load(&mut Fixtures);

        let project = store.find_or_create_project("/home/me/dotfiles".into());

        assert_eq!(project, ProjectId(1));
        assert_eq!(store.projects().len(), 2);
    }
}

mod picker {
    use std::path::PathBuf;

    use super::new_sessions::Fixtures;
    use lodestar_core::picker::{PickerEntry, PickerList};
    use lodestar_core::session::{ProjectId, SessionId, SessionStore};
    use lodestar_core::window::WindowModel;

    #[test]
    fn the_pickers_open_list_is_the_unshown_sessions_newest_first() {
        let mut store = SessionStore::load(&mut Fixtures);
        let mut model = WindowModel::new();
        let created = store.create_session(ProjectId(1));
        model.open_session(SessionId(0), model.focused_pane());

        let list = PickerList::new(&store, &model, "");

        let all: Vec<SessionId> = store
            .sessions_newest_first()
            .iter()
            .map(|session| session.id)
            .collect();
        assert_eq!(list.sessions(), model.not_shown(&all));
        assert_eq!(list.sessions(), vec![created, SessionId(1)]);
    }

    #[test]
    fn the_picker_offers_recent_folders_then_browse() {
        let store = SessionStore::load(&mut Fixtures);
        let model = WindowModel::new();

        let list = PickerList::new(&store, &model, "");

        assert_eq!(list.folders(), store.recent_folders());
        assert_eq!(
            list.entries(),
            vec![
                PickerEntry::Session(SessionId(0)),
                PickerEntry::Session(SessionId(1)),
                PickerEntry::Folder(PathBuf::from("/home/me/code/lodestar")),
                PickerEntry::Folder(PathBuf::from("/home/me/dotfiles")),
                PickerEntry::Browse,
            ]
        );
    }
}

mod picker_filter {
    use std::path::PathBuf;

    use super::new_sessions::Fixtures;
    use lodestar_core::picker::{PickerEntry, PickerList};
    use lodestar_core::session::{SessionId, SessionStore};
    use lodestar_core::window::WindowModel;

    #[test]
    fn typing_filters_sessions_by_title_and_folders_by_path_ignoring_case() {
        let store = SessionStore::load(&mut Fixtures);
        let model = WindowModel::new();

        let list = PickerList::new(&store, &model, "DOT");

        assert_eq!(
            list.entries(),
            vec![
                PickerEntry::Folder(PathBuf::from("/home/me/dotfiles")),
                PickerEntry::Browse,
            ]
        );
        let list = PickerList::new(&store, &model, "ci");
        assert_eq!(list.sessions(), [SessionId(1)]);
    }
}

mod picker_selection {
    use lodestar_core::picker::PickerState;

    #[test]
    fn moving_down_and_up_stays_within_the_list() {
        let mut picker = PickerState::default();
        assert_eq!(picker.selected(), 0);

        picker.move_up(3);
        assert_eq!(picker.selected(), 0);
        picker.move_down(3);
        picker.move_down(3);
        picker.move_down(3);
        assert_eq!(picker.selected(), 2);
        picker.move_up(3);
        assert_eq!(picker.selected(), 1);
    }

    #[test]
    fn typing_builds_the_filter_and_resets_the_selection() {
        let mut picker = PickerState::default();
        picker.move_down(5);

        picker.type_char('a');
        picker.type_char('b');
        picker.backspace();

        assert_eq!(picker.filter(), "a");
        assert_eq!(picker.selected(), 0);
    }
}

mod updates {
    use super::new_sessions::Fixtures;
    use lodestar_core::session::{
        SessionId, SessionStatus, SessionStore, SessionUpdate, TranscriptItem,
    };

    #[test]
    fn an_applied_transcript_item_is_appended_after_everything_so_far() {
        let mut store = SessionStore::load(&mut Fixtures);

        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::Assistant("First.".into()),
        });
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::Assistant("Second.".into()),
        });

        assert_eq!(
            store.session(SessionId(1)).unwrap().transcript.items(),
            [
                TranscriptItem::User("Hello".into()),
                TranscriptItem::Assistant("First.".into()),
                TranscriptItem::Assistant("Second.".into()),
            ]
        );
        assert_eq!(
            store
                .session(SessionId(0))
                .unwrap()
                .transcript
                .items()
                .len(),
            1
        );
    }

    #[test]
    fn an_applied_error_ends_the_transcript_so_the_user_sees_why_it_stopped() {
        let mut store = SessionStore::load(&mut Fixtures);

        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::Error("claude exited (code 1)".into()),
        });

        assert_eq!(
            store.session(SessionId(1)).unwrap().transcript.items(),
            [
                TranscriptItem::User("Hello".into()),
                TranscriptItem::Error("claude exited (code 1)".into()),
            ]
        );
    }

    #[test]
    fn an_applied_status_moves_the_session_to_it() {
        let mut store = SessionStore::load(&mut Fixtures);

        store.apply(SessionUpdate::Status {
            session: SessionId(0),
            status: SessionStatus::WaitingForInput,
        });

        assert_eq!(
            store.session(SessionId(0)).unwrap().status,
            SessionStatus::WaitingForInput
        );
        assert_eq!(
            store.session(SessionId(1)).unwrap().status,
            SessionStatus::Done
        );
    }

    #[test]
    fn an_update_for_an_unknown_session_changes_nothing() {
        let mut store = SessionStore::load(&mut Fixtures);
        let before = store.clone();

        store.apply(SessionUpdate::Status {
            session: SessionId(99),
            status: SessionStatus::Done,
        });

        assert_eq!(
            store.sessions_newest_first(),
            before.sessions_newest_first()
        );
    }
}

mod sending {
    use super::new_sessions::Fixtures;
    use lodestar_core::session::{
        ProjectId, SendRefusal, SessionId, SessionStatus, SessionStore, TranscriptItem,
    };

    #[test]
    fn a_message_sent_to_a_waiting_session_is_in_its_transcript_and_it_runs() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        let sent = store.send(session, "Add a README.");

        assert_eq!(sent, Ok(()));
        let session = store.session(session).unwrap();
        assert_eq!(
            session.transcript.items(),
            [TranscriptItem::User("Add a README.".into())]
        );
        assert_eq!(session.status, SessionStatus::Running);
    }

    #[test]
    fn a_running_session_refuses_a_message_and_keeps_its_transcript() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));
        store.send(session, "First.").unwrap();

        let sent = store.send(session, "Second.");

        assert_eq!(sent, Err(SendRefusal::AgentRunning));
        assert_eq!(
            store.session(session).unwrap().transcript.items(),
            [TranscriptItem::User("First.".into())]
        );
    }

    #[test]
    fn a_done_session_refuses_a_message() {
        let mut store = SessionStore::load(&mut Fixtures);

        let sent = store.send(SessionId(0), "Anyone there?");

        assert_eq!(sent, Err(SendRefusal::SessionDone));
        assert_eq!(
            store.session(SessionId(0)).unwrap().status,
            SessionStatus::Done
        );
    }

    #[test]
    fn a_blank_message_is_not_sent() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        let sent = store.send(session, "  \n ");

        assert_eq!(sent, Err(SendRefusal::Blank));
        assert!(
            store
                .session(session)
                .unwrap()
                .transcript
                .items()
                .is_empty()
        );
        assert_eq!(
            store.session(session).unwrap().status,
            SessionStatus::WaitingForInput
        );
    }

    #[test]
    fn a_message_for_an_unknown_session_is_refused() {
        let mut store = SessionStore::load(&mut Fixtures);

        assert_eq!(
            store.send(SessionId(99), "Hello?"),
            Err(SendRefusal::UnknownSession)
        );
    }

    #[test]
    fn can_send_answers_without_sending() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        assert_eq!(store.can_send(session), Ok(()));
        assert_eq!(store.can_send(SessionId(0)), Err(SendRefusal::SessionDone));
        assert!(
            store
                .session(session)
                .unwrap()
                .transcript
                .items()
                .is_empty()
        );
    }

    #[test]
    fn each_refusal_explains_itself_in_one_line() {
        assert_eq!(
            SendRefusal::AgentRunning.reason(),
            "Agent is running; wait for it to finish"
        );
        assert_eq!(SendRefusal::SessionDone.reason(), "Session is done");
        for refusal in [
            SendRefusal::AgentRunning,
            SendRefusal::SessionDone,
            SendRefusal::Blank,
            SendRefusal::UnknownSession,
        ] {
            assert!(!refusal.reason().contains('\n'));
        }
    }
}
