//! Minimal line-based driver for manual testing.
//!
//! Usage: `repl <user_data_dir>`, with `DUANYAN_LIBRIME_PATH` and
//! `DUANYAN_RIME_SHARED_DIR` set. Each input line is fed to rime key by key;
//! `{Name}` sends a rime key such as `{space}` or `{Shift_L}`, and
//! `{Shift_L}` style bare modifiers are sent as press + release. After each
//! line the commit, preedit and candidates are printed.

use std::io::BufRead;
use std::sync::Arc;

use rime_dl::keysym::is_modifier_keycode;
use rime_dl::{Library, Rime, RimeKey, Traits};

fn main() {
    let user = std::env::args()
        .nth(1)
        .expect("usage: repl <user_data_dir>");
    let lib = Arc::new(
        Library::open(std::env::var("DUANYAN_LIBRIME_PATH").expect("DUANYAN_LIBRIME_PATH"))
            .expect("load librime"),
    );
    let shared = std::env::var("DUANYAN_RIME_SHARED_DIR").expect("DUANYAN_RIME_SHARED_DIR");
    // glog does not create its log directory.
    let log_dir = std::env::temp_dir().join("duanyan-repl-log");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    let rime = Rime::init(
        lib,
        &Traits {
            shared_data_dir: shared.into(),
            user_data_dir: user.clone().into(),
            log_dir: Some(log_dir),
            min_log_level: 0,
            app_name: "rime.duanyan-repl".into(),
            distribution_name: "Duanyan".into(),
            distribution_code_name: "duanyan".into(),
            distribution_version: "repl".into(),
            staging_dir: None,
            prebuilt_data_dir: None,
        },
        Some(Box::new(|n| eprintln!("[notify] {n:?}"))),
    )
    .expect("init");
    if rime.start_maintenance(false) {
        rime.join_maintenance_thread();
    }
    let session = rime.create_session().expect("session");
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let mut rest = line.as_str();
        while !rest.is_empty() {
            let key = if let Some(stripped) = rest.strip_prefix('{') {
                let end = stripped.find('}').expect("unterminated {");
                let k = RimeKey::parse(&stripped[..end]).expect("bad key");
                rest = &stripped[end + 1..];
                k
            } else {
                let c = rest.chars().next().unwrap();
                rest = &rest[c.len_utf8()..];
                RimeKey::new(c as u32, 0)
            };
            let handled = session.process_key(key);
            if is_modifier_keycode(key.keycode) && !key.is_release() {
                session.process_key(key.released());
            }
            if !handled {
                println!("unhandled: {key}");
            }
        }
        if let Some(commit) = session.take_commit() {
            println!("commit: {commit}");
        }
        let ctx = session.context().unwrap_or_default();
        if let Some(c) = ctx.composition {
            println!("preedit: {}  (input: {:?})", c.preedit, session.input());
        }
        let cands: Vec<_> = ctx
            .menu
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}.{}", i + 1, c.text))
            .collect();
        if !cands.is_empty() {
            println!("page {}: {}", ctx.menu.page_no, cands.join(" "));
        }
    }
}
