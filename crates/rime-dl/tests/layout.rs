//! Compares the hand-written FFI structs against `rime_api.h` by compiling a
//! small C program that prints `sizeof`/`offsetof`. Runs only when
//! `RIME_INCLUDE_DIR` is set (the nix devShell exports it).

use std::fmt::Write as _;
use std::process::Command;

use rime_dl::ffi;

macro_rules! layouts {
    ($($ty:ident { $($field:ident),* $(,)? })*) => {{
        let mut c = String::new();
        let mut rust = Vec::new();
        $(
            writeln!(c, "printf(\"{} size %zu\\n\", sizeof({}));", stringify!($ty), stringify!($ty)).unwrap();
            rust.push(format!("{} size {}", stringify!($ty), std::mem::size_of::<ffi::$ty>()));
            $(
                writeln!(
                    c,
                    "printf(\"{}.{} %zu\\n\", offsetof({}, {}));",
                    stringify!($ty), stringify!($field), stringify!($ty), c_field(stringify!($field)),
                ).unwrap();
                rust.push(format!(
                    "{}.{} {}",
                    stringify!($ty),
                    stringify!($field),
                    std::mem::offset_of!(ffi::$ty, $field),
                ));
            )*
        )*
        (c, rust)
    }};
}

/// Rust field names that differ from the C header.
fn c_field(name: &str) -> &str {
    match name {
        "str_" => "str",
        other => other,
    }
}

#[test]
fn ffi_layout_matches_header() {
    let Ok(include) = std::env::var("RIME_INCLUDE_DIR") else {
        eprintln!("RIME_INCLUDE_DIR not set; skipping");
        return;
    };
    let (body, expected) = layouts! {
        RimeTraits { shared_data_dir, user_data_dir, distribution_name, distribution_code_name,
            distribution_version, app_name, modules, min_log_level, log_dir, prebuilt_data_dir,
            staging_dir }
        RimeComposition { length, cursor_pos, sel_start, sel_end, preedit }
        RimeCandidate { text, comment, reserved }
        RimeMenu { page_size, page_no, is_last_page, highlighted_candidate_index, num_candidates,
            candidates, select_keys }
        RimeCommit { text }
        RimeContext { composition, menu, commit_text_preview, select_labels }
        RimeStatus { schema_id, schema_name, is_disabled, is_composing, is_ascii_mode,
            is_full_shape, is_simplified, is_traditional, is_ascii_punct }
        RimeCandidateListIterator { ptr, index, candidate }
        RimeConfig { ptr }
        RimeConfigIterator { list, map, index, key, path }
        RimeSchemaListItem { schema_id, name, reserved }
        RimeSchemaList { size, list }
        RimeStringSlice { str_, length }
        RimeApi { setup, set_notification_handler, initialize, finalize, start_maintenance,
            is_maintenance_mode, join_maintenance_thread, deployer_initialize, prebuild, deploy,
            deploy_schema, deploy_config_file, sync_user_data, create_session, find_session,
            destroy_session, cleanup_stale_sessions, cleanup_all_sessions, process_key,
            commit_composition, clear_composition, get_commit, free_commit, get_context,
            free_context, get_status, free_status, set_option, get_option, set_property,
            get_property, get_schema_list, free_schema_list, get_current_schema, select_schema,
            schema_open, config_open, config_close, config_get_bool, config_get_int,
            config_get_double, config_get_string, config_get_cstring, config_update_signature,
            config_begin_map, config_next, config_end, simulate_key_sequence, register_module,
            find_module, run_task, get_shared_data_dir, get_user_data_dir, get_sync_dir,
            get_user_id, get_user_data_sync_dir, config_init, config_load_string, config_set_bool,
            config_set_int, config_set_double, config_set_string, config_get_item,
            config_set_item, config_clear, config_create_list, config_create_map,
            config_list_size, config_begin_list, get_input, get_caret_pos, select_candidate,
            get_version, set_caret_pos, select_candidate_on_current_page, candidate_list_begin,
            candidate_list_next, candidate_list_end, user_config_open, candidate_list_from_index,
            get_prebuilt_data_dir, get_staging_dir, commit_proto, context_proto, status_proto,
            get_state_label, delete_candidate, delete_candidate_on_current_page,
            get_state_label_abbreviated, set_input, get_shared_data_dir_s, get_user_data_dir_s,
            get_prebuilt_data_dir_s, get_staging_dir_s, get_sync_dir_s, highlight_candidate,
            highlight_candidate_on_current_page, change_page }
    };

    let dir = std::env::temp_dir().join(format!("rime-dl-layout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("layout.c");
    let exe = dir.join("layout");
    std::fs::write(
        &src,
        format!(
            "#include <stdio.h>\n#include <stddef.h>\n#include <rime_api.h>\n\
             int main(void) {{\n{body}return 0;\n}}\n"
        ),
    )
    .unwrap();
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let status = Command::new(cc)
        .arg("-I")
        .arg(&include)
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .status()
        .expect("failed to run the C compiler");
    assert!(status.success(), "compiling the layout probe failed");
    let out = Command::new(&exe).output().unwrap();
    let actual: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(actual, expected);
}
