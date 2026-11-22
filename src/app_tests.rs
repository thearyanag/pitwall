use super::*;
use crate::state::{Focus, TaskState, ViewMode};
use crate::ui::{conversation_lines, draw as draw_ui, Palette};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;
fn app() -> App {
    App::new(Config {
        team: "test".into(),
        board: String::new(),
        demo: true,
        node: "node".into(),
        adapter: PathBuf::from("unused"),
        data_dir: None,
    })
}
fn populated(tag: &str) -> (App, PathBuf) {
    let mut app = app();
    let dir = std::env::temp_dir().join(format!("sprout-ui-{tag}-{}", std::process::id()));
    app.config.adapter = dir.join("board-runtime/adapter.mjs");
    app.config.demo = false;
    app.loaded = true;
    app.channels = vec![Channel {
        path: "general".into(),
    }];
    app.items = vec![Item {
        id: "task-a".into(),
        title: "First task".into(),
        state: TaskState::Completed,
        lease_until: None,
        author: "Test".into(),
        replies: 0,
        labels: vec![],
    }];
    app.ensure_composer();
    (app, dir)
}
fn click(app: &mut App, action: Action) {
    let rect = app.ui.hits.iter().find(|(_, a)| a == &action).unwrap().0;
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + 1,
        row: rect.y + 1,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn model_picker_applies_saves_and_pins_selection_without_changing_the_agent() {
    let (mut app, dir) = populated("model-picker");
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    click(&mut app, Action::Models);
    assert!(app.model_picker.as_ref().unwrap().loading);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(!app.new_task);
    app.apply(Response{generation:app.generation, request:Request::Models{provider:"codex".into()}, data:Ok(json!({"provider":"codex","models":[{"id":"model-a","label":"Model A"},{"id":"model-b","label":"Model B"}]}))});
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    click(&mut app, Action::ChooseModel(2));
    assert_eq!(
        app.composer.as_ref().unwrap().draft.model.as_deref(),
        Some("model-b")
    );
    assert_eq!(app.composer.as_ref().unwrap().draft.provider, "codex");
    app.open_composer();
    app.open_reply();
    assert_eq!(
        app.composer.as_ref().unwrap().draft.model.as_deref(),
        Some("model-b")
    );
    app.composer
        .as_mut()
        .unwrap()
        .insert("Continue with this model");
    app.submit_draft();
    assert!(matches!(&app.pending,Some(Request::Reply{model:Some(model),..}) if model=="model-b"));
    app.action(Action::Models);
    assert!(app.model_picker.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn agent_picker_switches_replies_and_preserves_saved_choices() {
    let (mut app, dir) = populated("agent-picker");
    app.providers = vec![
        crate::board::AgentInfo {
            id: "codex".into(),
            label: "Codex".into(),
            installed: true,
        },
        crate::board::AgentInfo {
            id: "custom-agent".into(),
            label: "Custom agent".into(),
            installed: true,
        },
    ];
    app.open_composer();
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    click(&mut app, Action::NextAgent);
    assert_eq!(
        app.composer.as_ref().unwrap().draft.provider,
        "custom-agent"
    );
    app.composer.as_mut().unwrap().insert("Use this agent");
    app.save_draft();
    app.open_reply();
    app.open_composer();
    assert_eq!(
        app.composer.as_ref().unwrap().draft.provider,
        "custom-agent"
    );
    app.items[0].labels = vec!["pitwall.agent=codex".into()];
    app.open_reply();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    click(&mut app, Action::NextAgent);
    assert_eq!(
        app.composer.as_ref().unwrap().draft.provider,
        "custom-agent"
    );
    app.composer
        .as_mut()
        .unwrap()
        .insert("Review it with another agent");
    app.save_draft();
    app.open_composer();
    app.open_reply();
    assert_eq!(
        app.composer.as_ref().unwrap().draft.provider,
        "custom-agent"
    );
    app.submit_draft();
    assert!(
        matches!(&app.pending, Some(Request::Reply { provider, .. }) if provider == "custom-agent")
    );
    app.action(Action::NextAgent);
    assert_eq!(
        app.composer.as_ref().unwrap().draft.provider,
        "custom-agent",
        "submitted choice must remain pinned"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn agent_picker_new_task_selection_is_sent() {
    let (mut app, dir) = populated("new-agent");
    app.open_composer();
    app.composer.as_mut().unwrap().draft.provider = "custom-agent".into();
    app.composer.as_mut().unwrap().insert("Do the task");
    app.submit_draft();
    assert!(
        matches!(&app.pending, Some(Request::Create { provider, .. }) if provider == "custom-agent")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn approvals_are_scoped_clickable_and_block_underlying_navigation() {
    let (mut app, dir) = populated("approvals");
    let path = app.config.state_dir().join("approvals");
    std::fs::create_dir_all(&path).unwrap();
    let id = "00000000-0000-0000-0000-000000000001";
    let request = json!({ "id":id, "title":"Edit README", "detail":"line\n".repeat(100),
        "options":[{"id":"native-allow", "label":"Allow once"},{"id":"native-deny", "label":"Deny"}],
        "provider":"Test", "task":"task-a", "team":"test", "board":"General", "expires":u64::MAX });
    std::fs::write(path.join(format!("{id}.request.json")), request.to_string()).unwrap();
    app.approval = crate::approvals::Approval::read(&app.config);
    assert!(app.approval.is_some());
    let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app
        .ui
        .hits
        .iter()
        .all(|(_, a)| matches!(a, Action::Approval(_))));
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(!app.new_task);
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert!(app.approval_scroll > 0);
    click(&mut app, Action::Approval("native-deny".into()));
    let response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join(format!("{id}.response.json"))).unwrap())
            .unwrap();
    assert_eq!(response["choice"], "native-deny");
    assert!(crate::approvals::Approval::read(&app.config).is_none());
    app.config.team = "other".into();
    std::fs::remove_file(path.join(format!("{id}.response.json"))).unwrap();
    assert!(crate::approvals::Approval::read(&app.config).is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
fn refresh_scroll_test(app: &mut App, lines: usize) {
    app.apply(Response {
        generation: app.generation,
        request: Request::Thread {
            id: "task-a".into(),
        },
        data: Ok(json!({
            "id":"task-a", "state":"claimed", "details":[], "reply_count":0,
            "messages":[{"id":"task-a", "author":"Codex", "time":"",
                "text": (0..lines).map(|n| format!("Reply line {n}\n")).collect::<String>(),
                "attachments":[]}]
        })),
    });
}
#[test]
fn conversation_follows_refreshes_pauses_on_wheel_and_resumes_at_bottom() {
    let (mut app, dir) = populated("follow-wheel");
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    refresh_scroll_test(&mut app, 80);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app.max_scroll > 0);
    assert_eq!(app.scroll, app.max_scroll);
    let first_bottom = app.scroll;
    refresh_scroll_test(&mut app, 100);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app.scroll > first_bottom);
    assert_eq!(app.scroll, app.max_scroll);
    let wheel = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: app.ui.conversation.x + 1,
        row: app.ui.conversation.y + 1,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(wheel);
    let reading_position = app.scroll;
    assert!(!app.follow_latest);
    refresh_scroll_test(&mut app, 120);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert_eq!(app.scroll, reading_position);
    // A resize must not resume following or move a paused reading position.
    terminal.backend_mut().resize(140, 40);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(!app.follow_latest);
    assert_eq!(app.scroll, reading_position);
    app.scroll = app.max_scroll - 2;
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        ..wheel
    });
    assert!(app.follow_latest);
    refresh_scroll_test(&mut app, 140);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert_eq!(app.scroll, app.max_scroll);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn keyboard_reading_pauses_and_end_resumes_following() {
    let (mut app, dir) = populated("follow-keys");
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    for code in [
        KeyCode::Up,
        KeyCode::Char('k'),
        KeyCode::PageUp,
        KeyCode::Home,
        KeyCode::Char('g'),
    ] {
        app.focus = Focus::Conversation;
        app.follow_latest = true;
        refresh_scroll_test(&mut app, 80);
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        assert!(!app.follow_latest, "{code:?} should pause following");
        let reading_position = app.scroll;
        refresh_scroll_test(&mut app, 100);
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        assert_eq!(app.scroll, reading_position);
        app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        assert!(app.follow_latest);
        assert_eq!(app.scroll, app.max_scroll);
    }
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT));
    assert!(app.follow_latest);
    app.focus = Focus::Composer;
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert!(
        app.follow_latest,
        "editing the composer must not pause following"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn visible_composer_and_mouse_comment_send_keep_the_mode_pinned() {
    let (mut app, dir) = populated("comment");
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app.ui.editor.height >= 3);
    click(&mut app, Action::Mode(false));
    assert!(!app.composer.as_ref().unwrap().draft.send_to_agent);
    app.focus = Focus::Composer;
    for c in "Thanks!".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    click(&mut app, Action::Send);
    assert!(matches!(
        app.pending,
        Some(Request::Reply {
            run_worker: false,
            ..
        })
    ));
    let request = app.pending.clone().unwrap();
    let generation = app.generation;
    app.action(Action::Mode(true));
    assert!(!app.composer.as_ref().unwrap().draft.send_to_agent);
    app.apply(Response{generation,request,data:Ok(json!({"channel":"general","item":{"id":"task-a","title":"First task","state":"completed","author":"Test","replies":1,"labels":[]}}))});
    assert!(app.composer.as_ref().unwrap().draft.send_to_agent);
    assert!(app.composer.as_ref().unwrap().draft.text.is_empty());
    assert!(app.toast.as_ref().unwrap().0.contains("not started"));
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn switching_between_new_task_and_reply_preserves_each_draft() {
    let (mut app, dir) = populated("switch");
    app.focus = Focus::Composer;
    app.composer.as_mut().unwrap().insert("Reply draft");
    app.save_draft();
    app.open_composer();
    assert!(app.composer.as_ref().unwrap().draft.thread.is_none());
    app.composer.as_mut().unwrap().insert("New task draft");
    app.save_draft();
    app.open_reply();
    assert_eq!(app.composer.as_ref().unwrap().draft.text, "Reply draft");
    app.open_composer();
    assert_eq!(app.composer.as_ref().unwrap().draft.text, "New task draft");
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn inline_controls_fit_at_supported_sizes() {
    let (mut app, dir) = populated("layout");
    for (width, height) in [(100, 28), (120, 36), (210, 60)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        for (rect, _) in &app.ui.hits {
            assert!(rect.right() <= width && rect.bottom() <= height);
            assert!(rect.height >= 3);
        }
        assert!(app.ui.hits.iter().any(|(_, a)| a == &Action::Attach));
        assert!(app.ui.hits.iter().any(|(_, a)| a == &Action::Mode(false)));
        assert!(app.ui.editor.width >= 30 && app.ui.editor.height >= 3);
    }
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn help_toggles_closes_and_allows_quitting() {
    let mut app = app();
    for (code, expected) in [
        (KeyCode::Char('?'), true),
        (KeyCode::Char('?'), false),
        (KeyCode::Char('?'), true),
        (KeyCode::Esc, false),
        (KeyCode::Esc, false),
    ] {
        assert!(!app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)));
        assert_eq!(app.help, expected);
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
    assert!(app.help);
    assert!(app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
    assert!(app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Tasks);
}
#[test]
fn help_blocks_underlying_navigation_and_actions() {
    let mut app = app();
    app.channels = serde_json::from_value(json!([
        {"path":"first"}, {"path":"second"}, {"path":"third"}
    ]))
    .unwrap();
    app.items = (0..3)
        .map(|index| Item {
            id: index.to_string(),
            title: format!("Task {index}"),
            state: TaskState::Ready,
            lease_until: None,
            author: "Test".into(),
            replies: 0,
            labels: vec![],
        })
        .collect();
    app.channel = 1;
    app.item = 1;
    app.scroll = 5;
    app.max_scroll = 20;
    app.next = Some("next".into());
    app.help = true;
    for focus in [Focus::Channels, Focus::Tasks, Focus::Conversation] {
        app.focus = focus;
        for code in [
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Enter,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
        ]
        .into_iter()
        .chain("hljk gGrvdmti".chars().map(KeyCode::Char))
        {
            assert!(!app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)));
            assert_eq!(
                (app.focus, app.channel, app.item, app.scroll),
                (focus, 1, 1, 5)
            );
            assert_eq!(app.generation, 0);
            assert_eq!(app.mode, ViewMode::Tasks);
            assert!(app.config.demo);
            assert!(!app.light);
        }
    }
}
#[test]
fn help_renders_centered_in_both_themes_and_closes_cleanly() {
    for light in [false, true] {
        for (width, height) in [(100, 28), (120, 36), (160, 44)] {
            let mut app = app();
            app.light = light;
            let p = Palette::new(light);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
            let original = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(original.contains("? help"));
            app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
            terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            let x = (width - 76) / 2;
            let y = (height - 19) / 2;
            assert_eq!(buffer[(x, y)].symbol(), "╭");
            assert_eq!(buffer[(x + 75, y + 18)].symbol(), "╯");
            assert_eq!(buffer[(x, y)].fg, p.focus);
            assert_eq!(buffer[(x + 1, y + 1)].fg, p.text);
            for row in y..y + 19 {
                for col in x..x + 76 {
                    assert_eq!(buffer[(col, row)].bg, p.raised);
                }
            }
            let text = buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            for label in [
                "Keyboard help",
                "Tab / Shift-Tab",
                "h / l, Enter",
                "j / k",
                "Space / PgDn / PgUp",
                "g / G, Home / End",
                "Refresh channels",
                "tasks / all threads",
                "Next task page / older threads",
                "light/dark",
                "demo/live",
                "q / Ctrl-C",
                "? / Esc",
                "New task",
                "Comment only skips the agent",
            ] {
                assert!(text.contains(label), "missing {label} at {width}x{height}");
            }
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
            let closed = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(!closed.contains("Keyboard help"));
            assert!(closed.contains("Channels"));
            assert!(closed.contains("? help"));
        }
    }
}
#[test]
fn late_reply_cannot_replace_current_selection() {
    let mut app = app();
    app.generation = 9;
    app.apply(Response {
        generation: 8,
        request: Request::Thread { id: "old".into() },
        data: Ok(json!({"id":"old","messages":[],"state":"ready","details":[]})),
    });
    assert!(app.thread.is_none());
    assert!(app.error.is_none());
}
#[test]
fn error_is_distinct_from_empty_and_does_not_erase_known_conversation() {
    let mut app = app();
    app.generation = 1;
    app.thread = Some(Conversation {
        id: "current".into(),
        messages: vec![],
        state: TaskState::Ready,
        lease_until: None,
        details: vec![],
        reply_count: 0,
    });
    app.apply(Response {
        generation: 1,
        request: Request::Thread {
            id: "current".into(),
        },
        data: Err("Access denied".into()),
    });
    assert_eq!(app.thread.as_ref().unwrap().id, "current");
    assert_eq!(app.error.as_deref(), Some("Access denied"));
    let lines = conversation_lines(&app, &Palette::new(false));
    assert!(lines[0].to_string().contains("showing previous data"));
}
#[test]
fn render_empty_and_tiny_terminals_without_panicking() {
    let mut app = app();
    app.loaded = true;
    app.identity = "Test user".into();
    for (width, height) in [(1, 1), (50, 12), (100, 28), (120, 36)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        if width >= 100 {
            assert!(text.contains("Channels"));
            assert!(text.contains("no visible channels"));
        }
    }
}

#[test]
fn keyboard_and_button_use_the_same_send_and_view_actions() {
    for (label, send_key) in [
        (
            "control-s",
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        ),
        (
            "control-enter",
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
        ),
        (
            "command-enter",
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER),
        ),
    ] {
        let (mut app, dir) = populated(label);
        app.focus = Focus::Composer;
        app.handle_key(send_key);
        assert!(app.pending.is_none(), "empty drafts must not send");
        app.composer.as_mut().unwrap().insert("Please review this.");
        app.handle_key(send_key);
        assert!(matches!(
            app.pending,
            Some(Request::Reply {
                run_worker: true,
                ..
            })
        ));
        assert!(app.composer.as_ref().unwrap().draft.locked);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
    let (mut keyboard, a) = populated("view-key");
    let (mut button, b) = populated("view-button");
    keyboard.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    button.action(Action::View);
    assert_eq!(keyboard.mode, button.mode);
    assert_eq!(keyboard.items.len(), button.items.len());
    assert!(matches!(keyboard.pending, Some(Request::List { .. })));
    assert!(matches!(button.pending, Some(Request::List { .. })));
    drop((keyboard, button));
    let _ = std::fs::remove_dir_all(a);
    let _ = std::fs::remove_dir_all(b);
}

#[test]
fn help_blocks_mouse_wheel_navigation_too() {
    let (mut app, dir) = populated("help-wheel");
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    refresh_scroll_test(&mut app, 80);
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    app.help = true;
    let before = app.scroll;
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: app.ui.conversation.x + 1,
        row: app.ui.conversation.y + 1,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.scroll, before);
    assert!(app.follow_latest);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn feed_refresh_waits_for_pending_work_and_keeps_events_that_arrive_during_reads() {
    let (mut app, dir) = populated("feed-pending");
    refresh_scroll_test(&mut app, 80);
    let change = || Notice::Changed {
        channel: "general".into(),
        thread: "task-a".into(),
        parent: None,
        root: false,
    };
    app.follow_latest = false;
    app.pending = Some(Request::Thread {
        id: "task-a".into(),
    });
    app.apply_notice(change());
    app.refresh_from_feed();
    assert!(app.live_refresh.thread);
    app.pending = None;
    app.refresh_from_feed();
    assert!(matches!(app.pending, Some(Request::Thread { .. })));
    assert!(!app.live_refresh.thread);
    app.apply_notice(change());
    refresh_scroll_test(&mut app, 80);
    assert!(
        app.live_refresh.thread,
        "an event during a read must cause another snapshot"
    );
    app.refresh_from_feed();
    assert!(matches!(app.pending, Some(Request::Thread { .. })));
    assert!(!app.follow_latest);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn feed_routes_hidden_followups_and_reconnects_without_interrupting_sends() {
    let (mut app, dir) = populated("feed-reconnect");
    app.apply_notice(Notice::Changed {
        channel: "sprout-runs".into(),
        thread: "run-id".into(),
        parent: Some("task-a".into()),
        root: true,
    });
    assert!(app.live_refresh.thread);
    assert!(!app.live_refresh.list);
    app.apply_notice(Notice::State {
        state: FeedState::Retrying,
    });
    assert_eq!(app.feed_state, FeedState::Retrying);
    app.apply_notice(Notice::State {
        state: FeedState::Live,
    });
    app.composer.as_mut().unwrap().submitting = true;
    app.refresh_from_feed();
    assert!(app.pending.is_none());
    assert!(app.live_refresh.init);
    app.composer.as_mut().unwrap().submitting = false;
    app.refresh_from_feed();
    assert!(matches!(app.pending, Some(Request::Init)));
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn live_list_refresh_preserves_a_selected_task_outside_page_one() {
    let (mut app, dir) = populated("feed-selection");
    refresh_scroll_test(&mut app, 80);
    app.follow_latest = false;
    app.live_refresh.list = true;
    app.refresh_from_feed();
    let request = app.pending.clone().unwrap();
    app.apply(Response {
        generation: app.generation,
        request,
        data: Ok(json!({"items":[],"next":null})),
    });
    assert_eq!(app.item_id().as_deref(), Some("task-a"));
    assert!(!app.follow_latest);
    assert_eq!(
        app.composer.as_ref().unwrap().draft.thread.as_deref(),
        Some("task-a")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_expired_claim_refreshes_once_without_periodic_polling() {
    let (mut app, dir) = populated("feed-expiry");
    refresh_scroll_test(&mut app, 80);
    app.thread.as_mut().unwrap().lease_until = Some(1);
    app.items[0].state = TaskState::Claimed;
    app.items[0].lease_until = Some(1);
    app.expire_leases();
    assert_eq!(app.items[0].state, TaskState::Ready);
    assert!(app.live_refresh.thread);
    app.live_refresh.thread = false;
    app.expire_leases();
    assert!(!app.live_refresh.thread);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn logo_switches_theme_hides_under_overlays_and_falls_back_to_text() {
    let (mut app, dir) = populated("brand-logo");
    app.config.data_dir = Some(dir.join("persistent-state"));
    let assets = dir.join("assets/brand");
    std::fs::create_dir_all(&assets).unwrap();
    image::RgbaImage::from_pixel(64, 64, image::Rgba([244, 244, 241, 255]))
        .save(assets.join("pitwall-mark-on-dark.png"))
        .unwrap();
    image::RgbaImage::from_pixel(128, 64, image::Rgba([14, 15, 18, 255]))
        .save(assets.join("pitwall-mark-on-light.png"))
        .unwrap();
    app.image_picker.set_protocol_type(ProtocolType::Kitty);
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert_eq!(app.logo.as_ref().unwrap().frame_area.unwrap().height, 2);
    app.light = true;
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert_eq!(app.logo.as_ref().unwrap().frame_area.unwrap().height, 1);
    app.help = true;
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app.logo.as_ref().unwrap().frame_area.is_none());
    app.help = false;
    app.image_picker = Picker::halfblocks();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    assert!(app.logo.is_none());
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(text.contains("PITWALL"));
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}
