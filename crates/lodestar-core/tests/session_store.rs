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
        SessionId, SessionStatus, SessionStore, SessionUpdate, ToolCall, TranscriptItem,
    };

    fn pending_call(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: "Bash".into(),
            summary: "cargo test".into(),
            input: "cargo test".into(),
            output: None,
        }
    }

    fn call_with(id: &str, output: &str) -> ToolCall {
        ToolCall {
            output: Some(output.into()),
            ..pending_call(id)
        }
    }

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

    #[test]
    fn a_tool_result_fills_the_output_of_the_pending_call_with_its_id() {
        let mut store = SessionStore::load(&mut Fixtures);
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::ToolCall(pending_call("toolu_1")),
        });

        store.apply(SessionUpdate::ToolResult {
            session: SessionId(1),
            call_id: "toolu_1".into(),
            output: "4 passed".into(),
        });

        assert_eq!(
            store.session(SessionId(1)).unwrap().transcript.items(),
            [
                TranscriptItem::User("Hello".into()),
                TranscriptItem::ToolCall(call_with("toolu_1", "4 passed")),
            ]
        );
    }

    #[test]
    fn a_tool_result_still_finds_its_call_after_more_items_were_pushed() {
        let mut store = SessionStore::load(&mut Fixtures);
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::ToolCall(pending_call("toolu_1")),
        });
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::Assistant("Still running...".into()),
        });
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::ToolCall(pending_call("toolu_2")),
        });

        store.apply(SessionUpdate::ToolResult {
            session: SessionId(1),
            call_id: "toolu_1".into(),
            output: "4 passed".into(),
        });

        assert_eq!(
            store.session(SessionId(1)).unwrap().transcript.items(),
            [
                TranscriptItem::User("Hello".into()),
                TranscriptItem::ToolCall(call_with("toolu_1", "4 passed")),
                TranscriptItem::Assistant("Still running...".into()),
                TranscriptItem::ToolCall(pending_call("toolu_2")),
            ]
        );
    }

    #[test]
    fn an_ended_session_is_done_with_its_error_last_and_no_call_left_pending() {
        let mut store = SessionStore::load(&mut Fixtures);
        let before = store
            .session(SessionId(0))
            .unwrap()
            .transcript
            .items()
            .len();
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(0),
            item: TranscriptItem::ToolCall(pending_call("toolu_1")),
        });
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(0),
            item: TranscriptItem::ToolCall(call_with("toolu_2", "4 passed")),
        });

        store.apply(SessionUpdate::Ended {
            session: SessionId(0),
            error: "claude exited (code 1).".into(),
        });

        let session = store.session(SessionId(0)).unwrap();
        assert_eq!(session.status, SessionStatus::Done);
        assert_eq!(
            session.transcript.items()[before..],
            [
                TranscriptItem::ToolCall(call_with("toolu_1", "No result: the Agent exited.")),
                TranscriptItem::ToolCall(call_with("toolu_2", "4 passed")),
                TranscriptItem::Error("claude exited (code 1).".into()),
            ]
        );
    }

    #[test]
    fn every_update_names_its_session() {
        let updates = [
            SessionUpdate::TranscriptItem {
                session: SessionId(4),
                item: TranscriptItem::Assistant("Hi".into()),
            },
            SessionUpdate::Status {
                session: SessionId(4),
                status: SessionStatus::Done,
            },
            SessionUpdate::ToolResult {
                session: SessionId(4),
                call_id: "toolu_1".into(),
                output: "ok".into(),
            },
            SessionUpdate::Ended {
                session: SessionId(4),
                error: "gone".into(),
            },
        ];

        assert!(
            updates
                .iter()
                .all(|update| update.session() == SessionId(4))
        );
    }

    #[test]
    fn a_tool_result_for_an_unknown_call_changes_nothing() {
        let mut store = SessionStore::load(&mut Fixtures);
        store.apply(SessionUpdate::TranscriptItem {
            session: SessionId(1),
            item: TranscriptItem::ToolCall(pending_call("toolu_1")),
        });
        let before = store.clone();

        store.apply(SessionUpdate::ToolResult {
            session: SessionId(1),
            call_id: "toolu_404".into(),
            output: "lost".into(),
        });

        assert_eq!(
            store.sessions_newest_first(),
            before.sessions_newest_first()
        );
    }
}

mod sending {
    use std::time::Duration;

    use super::new_sessions::Fixtures;
    use lodestar_core::session::{
        ProjectId, SendRefusal, SessionId, SessionStatus, SessionStore, TranscriptItem,
    };

    #[test]
    fn a_message_sent_to_a_waiting_session_is_in_its_transcript_and_it_runs() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        let sent = store.send(session, "Add a README.", Duration::ZERO);

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
        store.send(session, "First.", Duration::ZERO).unwrap();

        let sent = store.send(session, "Second.", Duration::ZERO);

        assert_eq!(sent, Err(SendRefusal::AgentRunning));
        assert_eq!(
            store.session(session).unwrap().transcript.items(),
            [TranscriptItem::User("First.".into())]
        );
    }

    #[test]
    fn a_done_session_refuses_a_message() {
        let mut store = SessionStore::load(&mut Fixtures);

        let sent = store.send(SessionId(0), "Anyone there?", Duration::ZERO);

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

        let sent = store.send(session, "  \n ", Duration::ZERO);

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
            store.send(SessionId(99), "Hello?", Duration::ZERO),
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

mod turn_progress {
    use std::time::Duration;

    use super::new_sessions::Fixtures;
    use lodestar_core::session::{
        ProjectId, SessionId, SessionStatus, SessionStore, SessionUpdate,
    };
    use lodestar_core::turn::{Phase, TurnProgress};

    fn running(at: Duration) -> (SessionStore, SessionId) {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));
        store.send(session, "Go.", at).unwrap();
        (store, session)
    }

    fn progress(session: SessionId, phase: Phase, output_tokens: u64) -> SessionUpdate {
        SessionUpdate::Progress {
            session,
            phase,
            output_tokens,
        }
    }

    #[test]
    fn progress_moves_the_turn_to_its_phase_and_token_count_but_keeps_its_start() {
        let (mut store, session) = running(Duration::from_secs(5));

        store.apply(progress(session, Phase::Thinking, 120));
        store.apply(progress(session, Phase::Tool("Bash".into()), 340));

        assert_eq!(
            store.turn(session),
            Some(&TurnProgress {
                started: Duration::from_secs(5),
                phase: Phase::Tool("Bash".into()),
                output_tokens: 340,
            })
        );
    }

    #[test]
    fn the_turn_is_gone_once_the_session_waits_for_input_or_is_done_or_ended() {
        let ends = [
            |session| SessionUpdate::Status {
                session,
                status: SessionStatus::WaitingForInput,
            },
            |session| SessionUpdate::Status {
                session,
                status: SessionStatus::Done,
            },
            |session| SessionUpdate::Ended {
                session,
                error: "claude exited (code 1).".into(),
            },
        ];
        for end in ends {
            let (mut store, session) = running(Duration::ZERO);

            store.apply(end(session));

            assert_eq!(store.turn(session), None);
        }
    }

    #[test]
    fn progress_for_a_session_without_a_turn_is_dropped() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        store.apply(progress(session, Phase::Writing, 10));

        assert_eq!(store.turn(session), None);
    }

    #[test]
    fn a_session_already_running_at_startup_has_a_turn_from_the_start_of_the_clock() {
        let store = SessionStore::load(&mut super::OneSession);

        assert_eq!(
            store.turn(SessionId(0)),
            Some(&TurnProgress {
                started: Duration::ZERO,
                phase: Phase::Working,
                output_tokens: 0,
            })
        );
    }

    #[test]
    fn sending_a_message_starts_a_turn_that_is_working_from_that_moment() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));
        assert_eq!(store.turn(session), None);

        store
            .send(session, "Add a README.", Duration::from_secs(42))
            .unwrap();

        assert_eq!(
            store.turn(session),
            Some(&TurnProgress {
                started: Duration::from_secs(42),
                phase: Phase::Working,
                output_tokens: 0,
            })
        );
    }

    #[test]
    fn only_a_running_session_has_a_working_line_counting_from_its_turn_start() {
        let (mut store, session) = running(Duration::from_secs(5));
        store.apply(progress(session, Phase::Thinking, 1500));

        assert_eq!(
            store.working_line(session, Duration::from_secs(60)),
            Some("55s \u{b7} 1.5k tokens \u{b7} Thinking".into())
        );

        store.apply(SessionUpdate::Status {
            session,
            status: SessionStatus::WaitingForInput,
        });

        assert_eq!(store.working_line(session, Duration::from_secs(61)), None);
    }

    #[test]
    fn a_session_set_running_without_a_turn_just_says_it_is_working() {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));

        store.apply(SessionUpdate::Status {
            session,
            status: SessionStatus::Running,
        });

        assert_eq!(
            store.working_line(session, Duration::from_secs(9)),
            Some("Working".into())
        );
    }
}

mod suggestions {
    use std::time::Duration;

    use super::new_sessions::Fixtures;
    use lodestar_core::session::{
        ProjectId, SessionId, SessionStatus, SessionStore, SessionUpdate,
    };

    fn suggest(session: SessionId, text: &str) -> SessionUpdate {
        SessionUpdate::Suggestion {
            session,
            text: text.into(),
        }
    }

    fn waiting_session() -> (SessionStore, SessionId) {
        let mut store = SessionStore::load(&mut Fixtures);
        let session = store.create_session(ProjectId(1));
        (store, session)
    }

    #[test]
    fn a_waiting_session_keeps_the_suggestion_its_agent_made() {
        let (mut store, session) = waiting_session();

        store.apply(suggest(session, "run cargo test"));

        assert_eq!(store.suggestion(session), Some("run cargo test"));
    }

    #[test]
    fn a_session_whose_agent_never_suggests_has_no_suggestion() {
        let (store, session) = waiting_session();

        assert_eq!(store.suggestion(session), None);
    }

    #[test]
    fn sending_the_next_message_drops_the_suggestion() {
        let (mut store, session) = waiting_session();
        store.apply(suggest(session, "run cargo test"));

        store
            .send(session, "Fix the README.", Duration::ZERO)
            .unwrap();

        assert_eq!(store.suggestion(session), None);
    }

    #[test]
    fn a_suggestion_that_arrives_once_the_next_turn_runs_is_dropped() {
        let (mut store, session) = waiting_session();
        store.send(session, "Go.", Duration::ZERO).unwrap();

        store.apply(suggest(session, "run cargo test"));

        assert_eq!(store.suggestion(session), None);
    }

    #[test]
    fn a_session_that_runs_or_ends_loses_its_suggestion() {
        let leaving_waiting = [
            |session| SessionUpdate::Status {
                session,
                status: SessionStatus::Running,
            },
            |session| SessionUpdate::Ended {
                session,
                error: "claude exited (code 1).".into(),
            },
        ];
        for leave in leaving_waiting {
            let (mut store, session) = waiting_session();
            store.apply(suggest(session, "run cargo test"));

            store.apply(leave(session));

            assert_eq!(store.suggestion(session), None);
        }
    }

    #[test]
    fn a_newer_suggestion_replaces_the_older_one() {
        let (mut store, session) = waiting_session();
        store.apply(suggest(session, "run cargo test"));

        store.apply(suggest(session, "commit the fix"));

        assert_eq!(store.suggestion(session), Some("commit the fix"));
    }
}
