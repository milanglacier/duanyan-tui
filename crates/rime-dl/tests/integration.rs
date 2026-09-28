//! End-to-end checks against a real librime. Runs only when
//! `DUANYAN_LIBRIME_PATH` and `DUANYAN_RIME_SHARED_DIR` are set (the nix
//! devShell exports both). librime can be initialized once per process, so
//! everything lives in one test.

use std::sync::{Arc, Mutex};

use rime_dl::keysym::{mask, sym};
use rime_dl::{Library, Rime, RimeKey, Traits};

fn key(c: char) -> RimeKey {
    RimeKey::new(c as u32, 0)
}

#[test]
fn luna_pinyin_end_to_end() {
    let (Ok(lib_path), Ok(shared)) = (
        std::env::var("DUANYAN_LIBRIME_PATH"),
        std::env::var("DUANYAN_RIME_SHARED_DIR"),
    ) else {
        eprintln!("DUANYAN_LIBRIME_PATH / DUANYAN_RIME_SHARED_DIR not set; skipping");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("rime-dl-it-{}", std::process::id()));
    let user = tmp.join("user");
    std::fs::create_dir_all(&user).unwrap();
    // glog does not create its log directory.
    std::fs::create_dir_all(tmp.join("log")).unwrap();
    // Deploy a single schema to keep the test fast.
    std::fs::write(
        user.join("default.custom.yaml"),
        "patch:\n  schema_list:\n    - schema: luna_pinyin_simp\n",
    )
    .unwrap();

    let lib = Arc::new(Library::open(&lib_path).expect("load librime"));
    let notes = Arc::new(Mutex::new(Vec::new()));
    let sink = notes.clone();
    let rime = Rime::init(
        lib,
        &Traits {
            shared_data_dir: shared.into(),
            user_data_dir: user.clone(),
            log_dir: Some(tmp.join("log")),
            min_log_level: 2,
            app_name: "rime.duanyan-test".into(),
            distribution_name: "Duanyan".into(),
            distribution_code_name: "duanyan".into(),
            distribution_version: "test".into(),
            staging_dir: None,
            prebuilt_data_dir: None,
        },
        Some(Box::new(move |n| sink.lock().unwrap().push(n))),
    )
    .expect("init");

    assert!(rime.start_maintenance(true));
    rime.join_maintenance_thread();
    assert!(
        notes
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.message_type == "deploy" && n.message_value == "success"),
        "deploy notifications: {:?}",
        notes.lock().unwrap()
    );

    // The deployed build records its time, which the frontend's
    // modification check reads back.
    let mut user_cfg = rime.user_config("user").expect("user.yaml");
    assert!(user_cfg.get_int("var/last_build_time").unwrap_or(0) > 0);
    drop(user_cfg);

    let session = rime.create_session().expect("session");
    assert_eq!(
        session.current_schema().as_deref(),
        Some("luna_pinyin_simp")
    );

    // Tab is not consumed while nothing is being composed.
    assert!(!session.process_key(RimeKey::new(sym::TAB, 0)));

    for c in "nihao".chars() {
        assert!(session.process_key(key(c)));
    }
    let ctx = session.context().expect("context");
    let comp = ctx.composition.expect("composing");
    assert!(!comp.preedit.is_empty());
    assert!(
        ctx.menu.candidates.iter().any(|c| c.text == "你好"),
        "candidates: {:?}",
        ctx.menu.candidates
    );
    assert_eq!(session.input().as_deref(), Some("nihao"));
    assert!(session.status().unwrap().is_composing);

    assert!(session.process_key(RimeKey::new(sym::SPACE, 0)));
    assert_eq!(session.take_commit().as_deref(), Some("你好"));
    assert!(!session.status().unwrap().is_composing);

    // A lone Shift_L press + release toggles ascii_mode.
    assert!(!session.get_option("ascii_mode"));
    session.process_key(RimeKey::new(sym::SHIFT_L, 0));
    session.process_key(RimeKey::new(sym::SHIFT_L, mask::RELEASE));
    assert!(session.get_option("ascii_mode"));
    // In ascii mode letters are left to the frontend.
    assert!(!session.process_key(key('a')));
    session.set_option("ascii_mode", false);

    // Switch labels come from the schema.
    let label = session.state_label("ascii_mode", false, false);
    assert!(label.is_some(), "ascii_mode label");

    let mut schema = rime
        .schema_config("luna_pinyin_simp")
        .expect("schema config");
    assert!(schema.list_size("switches") > 0);
    drop(schema);

    drop(session);
    drop(rime);
    std::fs::remove_dir_all(&tmp).ok();
}
