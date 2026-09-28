//! Hand-written `#[repr(C)]` mirror of `rime_api.h`.
//!
//! Only `rime_get_api` is resolved from the shared library; every other entry
//! point is reached through the returned [`RimeApi`] function table. Fields
//! appended in newer librime releases must be gated with
//! [`api_has_member`] before use.

#![allow(non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_int, c_void};

pub type Bool = c_int;
pub type RimeSessionId = usize;

#[repr(C)]
pub struct RimeTraits {
    pub data_size: c_int,
    pub shared_data_dir: *const c_char,
    pub user_data_dir: *const c_char,
    pub distribution_name: *const c_char,
    pub distribution_code_name: *const c_char,
    pub distribution_version: *const c_char,
    pub app_name: *const c_char,
    pub modules: *const *const c_char,
    pub min_log_level: c_int,
    pub log_dir: *const c_char,
    pub prebuilt_data_dir: *const c_char,
    pub staging_dir: *const c_char,
}

#[repr(C)]
pub struct RimeComposition {
    pub length: c_int,
    pub cursor_pos: c_int,
    pub sel_start: c_int,
    pub sel_end: c_int,
    pub preedit: *mut c_char,
}

#[repr(C)]
pub struct RimeCandidate {
    pub text: *mut c_char,
    pub comment: *mut c_char,
    pub reserved: *mut c_void,
}

#[repr(C)]
pub struct RimeMenu {
    pub page_size: c_int,
    pub page_no: c_int,
    pub is_last_page: Bool,
    pub highlighted_candidate_index: c_int,
    pub num_candidates: c_int,
    pub candidates: *mut RimeCandidate,
    pub select_keys: *mut c_char,
}

#[repr(C)]
pub struct RimeCommit {
    pub data_size: c_int,
    pub text: *mut c_char,
}

#[repr(C)]
pub struct RimeContext {
    pub data_size: c_int,
    pub composition: RimeComposition,
    pub menu: RimeMenu,
    pub commit_text_preview: *mut c_char,
    pub select_labels: *mut *mut c_char,
}

#[repr(C)]
pub struct RimeStatus {
    pub data_size: c_int,
    pub schema_id: *mut c_char,
    pub schema_name: *mut c_char,
    pub is_disabled: Bool,
    pub is_composing: Bool,
    pub is_ascii_mode: Bool,
    pub is_full_shape: Bool,
    pub is_simplified: Bool,
    pub is_traditional: Bool,
    pub is_ascii_punct: Bool,
}

#[repr(C)]
pub struct RimeCandidateListIterator {
    pub ptr: *mut c_void,
    pub index: c_int,
    pub candidate: RimeCandidate,
}

#[repr(C)]
pub struct RimeConfig {
    pub ptr: *mut c_void,
}

#[repr(C)]
pub struct RimeConfigIterator {
    pub list: *mut c_void,
    pub map: *mut c_void,
    pub index: c_int,
    pub key: *const c_char,
    pub path: *const c_char,
}

#[repr(C)]
pub struct RimeSchemaListItem {
    pub schema_id: *mut c_char,
    pub name: *mut c_char,
    pub reserved: *mut c_void,
}

#[repr(C)]
pub struct RimeSchemaList {
    pub size: usize,
    pub list: *mut RimeSchemaListItem,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RimeStringSlice {
    pub str_: *const c_char,
    pub length: usize,
}

pub type RimeNotificationHandler = Option<
    unsafe extern "C" fn(
        context_object: *mut c_void,
        session_id: RimeSessionId,
        message_type: *const c_char,
        message_value: *const c_char,
    ),
>;

/// Opaque; duanyan never registers modules.
pub type RimeModule = c_void;
/// Opaque capnproto builder pointer.
pub type RimeProtoBuilder = c_void;

#[repr(C)]
pub struct RimeApi {
    pub data_size: c_int,

    pub setup: Option<unsafe extern "C" fn(traits: *mut RimeTraits)>,
    pub set_notification_handler:
        Option<unsafe extern "C" fn(handler: RimeNotificationHandler, context_object: *mut c_void)>,

    pub initialize: Option<unsafe extern "C" fn(traits: *mut RimeTraits)>,
    pub finalize: Option<unsafe extern "C" fn()>,
    pub start_maintenance: Option<unsafe extern "C" fn(full_check: Bool) -> Bool>,
    pub is_maintenance_mode: Option<unsafe extern "C" fn() -> Bool>,
    pub join_maintenance_thread: Option<unsafe extern "C" fn()>,

    pub deployer_initialize: Option<unsafe extern "C" fn(traits: *mut RimeTraits)>,
    pub prebuild: Option<unsafe extern "C" fn() -> Bool>,
    pub deploy: Option<unsafe extern "C" fn() -> Bool>,
    pub deploy_schema: Option<unsafe extern "C" fn(schema_file: *const c_char) -> Bool>,
    pub deploy_config_file:
        Option<unsafe extern "C" fn(file_name: *const c_char, version_key: *const c_char) -> Bool>,
    pub sync_user_data: Option<unsafe extern "C" fn() -> Bool>,

    pub create_session: Option<unsafe extern "C" fn() -> RimeSessionId>,
    pub find_session: Option<unsafe extern "C" fn(session_id: RimeSessionId) -> Bool>,
    pub destroy_session: Option<unsafe extern "C" fn(session_id: RimeSessionId) -> Bool>,
    pub cleanup_stale_sessions: Option<unsafe extern "C" fn()>,
    pub cleanup_all_sessions: Option<unsafe extern "C" fn()>,

    pub process_key: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, keycode: c_int, mask: c_int) -> Bool,
    >,
    pub commit_composition: Option<unsafe extern "C" fn(session_id: RimeSessionId) -> Bool>,
    pub clear_composition: Option<unsafe extern "C" fn(session_id: RimeSessionId)>,

    pub get_commit:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, commit: *mut RimeCommit) -> Bool>,
    pub free_commit: Option<unsafe extern "C" fn(commit: *mut RimeCommit) -> Bool>,
    pub get_context:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, context: *mut RimeContext) -> Bool>,
    pub free_context: Option<unsafe extern "C" fn(ctx: *mut RimeContext) -> Bool>,
    pub get_status:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, status: *mut RimeStatus) -> Bool>,
    pub free_status: Option<unsafe extern "C" fn(status: *mut RimeStatus) -> Bool>,

    pub set_option:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, option: *const c_char, value: Bool)>,
    pub get_option:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, option: *const c_char) -> Bool>,
    pub set_property: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, prop: *const c_char, value: *const c_char),
    >,
    pub get_property: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            prop: *const c_char,
            value: *mut c_char,
            buffer_size: usize,
        ) -> Bool,
    >,
    pub get_schema_list: Option<unsafe extern "C" fn(schema_list: *mut RimeSchemaList) -> Bool>,
    pub free_schema_list: Option<unsafe extern "C" fn(schema_list: *mut RimeSchemaList)>,
    pub get_current_schema: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            schema_id: *mut c_char,
            buffer_size: usize,
        ) -> Bool,
    >,
    pub select_schema:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, schema_id: *const c_char) -> Bool>,

    pub schema_open:
        Option<unsafe extern "C" fn(schema_id: *const c_char, config: *mut RimeConfig) -> Bool>,
    pub config_open:
        Option<unsafe extern "C" fn(config_id: *const c_char, config: *mut RimeConfig) -> Bool>,
    pub config_close: Option<unsafe extern "C" fn(config: *mut RimeConfig) -> Bool>,
    pub config_get_bool: Option<
        unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char, value: *mut Bool) -> Bool,
    >,
    pub config_get_int: Option<
        unsafe extern "C" fn(
            config: *mut RimeConfig,
            key: *const c_char,
            value: *mut c_int,
        ) -> Bool,
    >,
    pub config_get_double: Option<
        unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char, value: *mut f64) -> Bool,
    >,
    pub config_get_string: Option<
        unsafe extern "C" fn(
            config: *mut RimeConfig,
            key: *const c_char,
            value: *mut c_char,
            buffer_size: usize,
        ) -> Bool,
    >,
    pub config_get_cstring:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char) -> *const c_char>,
    pub config_update_signature:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, signer: *const c_char) -> Bool>,
    pub config_begin_map: Option<
        unsafe extern "C" fn(
            iterator: *mut RimeConfigIterator,
            config: *mut RimeConfig,
            key: *const c_char,
        ) -> Bool,
    >,
    pub config_next: Option<unsafe extern "C" fn(iterator: *mut RimeConfigIterator) -> Bool>,
    pub config_end: Option<unsafe extern "C" fn(iterator: *mut RimeConfigIterator)>,

    pub simulate_key_sequence: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, key_sequence: *const c_char) -> Bool,
    >,

    pub register_module: Option<unsafe extern "C" fn(module: *mut RimeModule) -> Bool>,
    pub find_module: Option<unsafe extern "C" fn(module_name: *const c_char) -> *mut RimeModule>,

    pub run_task: Option<unsafe extern "C" fn(task_name: *const c_char) -> Bool>,
    pub get_shared_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_sync_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_id: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_data_sync_dir: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,

    pub config_init: Option<unsafe extern "C" fn(config: *mut RimeConfig) -> Bool>,
    pub config_load_string:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, yaml: *const c_char) -> Bool>,
    pub config_set_bool: Option<
        unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char, value: Bool) -> Bool,
    >,
    pub config_set_int: Option<
        unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char, value: c_int) -> Bool,
    >,
    pub config_set_double: Option<
        unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char, value: f64) -> Bool,
    >,
    pub config_set_string: Option<
        unsafe extern "C" fn(
            config: *mut RimeConfig,
            key: *const c_char,
            value: *const c_char,
        ) -> Bool,
    >,
    pub config_get_item: Option<
        unsafe extern "C" fn(
            config: *mut RimeConfig,
            key: *const c_char,
            value: *mut RimeConfig,
        ) -> Bool,
    >,
    pub config_set_item: Option<
        unsafe extern "C" fn(
            config: *mut RimeConfig,
            key: *const c_char,
            value: *mut RimeConfig,
        ) -> Bool,
    >,
    pub config_clear:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char) -> Bool>,
    pub config_create_list:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char) -> Bool>,
    pub config_create_map:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char) -> Bool>,
    pub config_list_size:
        Option<unsafe extern "C" fn(config: *mut RimeConfig, key: *const c_char) -> usize>,
    pub config_begin_list: Option<
        unsafe extern "C" fn(
            iterator: *mut RimeConfigIterator,
            config: *mut RimeConfig,
            key: *const c_char,
        ) -> Bool,
    >,

    pub get_input: Option<unsafe extern "C" fn(session_id: RimeSessionId) -> *const c_char>,
    pub get_caret_pos: Option<unsafe extern "C" fn(session_id: RimeSessionId) -> usize>,
    pub select_candidate:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub get_version: Option<unsafe extern "C" fn() -> *const c_char>,
    pub set_caret_pos: Option<unsafe extern "C" fn(session_id: RimeSessionId, caret_pos: usize)>,
    pub select_candidate_on_current_page:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub candidate_list_begin: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            iterator: *mut RimeCandidateListIterator,
        ) -> Bool,
    >,
    pub candidate_list_next:
        Option<unsafe extern "C" fn(iterator: *mut RimeCandidateListIterator) -> Bool>,
    pub candidate_list_end: Option<unsafe extern "C" fn(iterator: *mut RimeCandidateListIterator)>,
    pub user_config_open:
        Option<unsafe extern "C" fn(config_id: *const c_char, config: *mut RimeConfig) -> Bool>,
    pub candidate_list_from_index: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            iterator: *mut RimeCandidateListIterator,
            index: c_int,
        ) -> Bool,
    >,
    pub get_prebuilt_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_staging_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub commit_proto: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, commit_builder: *mut RimeProtoBuilder),
    >,
    pub context_proto: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, context_builder: *mut RimeProtoBuilder),
    >,
    pub status_proto: Option<
        unsafe extern "C" fn(session_id: RimeSessionId, status_builder: *mut RimeProtoBuilder),
    >,
    pub get_state_label: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            option_name: *const c_char,
            state: Bool,
        ) -> *const c_char,
    >,
    pub delete_candidate:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub delete_candidate_on_current_page:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub get_state_label_abbreviated: Option<
        unsafe extern "C" fn(
            session_id: RimeSessionId,
            option_name: *const c_char,
            state: Bool,
            abbreviated: Bool,
        ) -> RimeStringSlice,
    >,
    pub set_input:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, input: *const c_char) -> Bool>,
    pub get_shared_data_dir_s: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,
    pub get_user_data_dir_s: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,
    pub get_prebuilt_data_dir_s: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,
    pub get_staging_dir_s: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,
    pub get_sync_dir_s: Option<unsafe extern "C" fn(dir: *mut c_char, buffer_size: usize)>,
    pub highlight_candidate:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub highlight_candidate_on_current_page:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, index: usize) -> Bool>,
    pub change_page:
        Option<unsafe extern "C" fn(session_id: RimeSessionId, backward: Bool) -> Bool>,
}

pub type RimeGetApiFn = unsafe extern "C" fn() -> *mut RimeApi;

/// `RIME_STRUCT_INIT`: `data_size` excludes the `data_size` field itself.
pub const fn struct_data_size<T>() -> c_int {
    (std::mem::size_of::<T>() - std::mem::size_of::<c_int>()) as c_int
}

/// `RIME_STRUCT_HAS_MEMBER` for a member at byte `offset`.
pub fn api_has_member(data_size: c_int, offset: usize) -> bool {
    (std::mem::size_of::<c_int>() as isize + data_size as isize) > offset as isize
}

/// Checks that `api->$field` exists in the loaded librime and is non-null,
/// mirroring `RIME_API_AVAILABLE`.
#[macro_export]
#[doc(hidden)]
macro_rules! api_fn {
    ($api:expr, $field:ident) => {{
        let api: &$crate::ffi::RimeApi = $api;
        if $crate::ffi::api_has_member(
            api.data_size,
            ::std::mem::offset_of!($crate::ffi::RimeApi, $field),
        ) {
            api.$field
        } else {
            None
        }
    }};
}
