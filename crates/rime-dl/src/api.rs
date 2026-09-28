//! Safe wrappers over the librime API table.
//!
//! Every value handed out is owned Rust data; no pointer into librime memory
//! outlives the call that produced it.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::keysym::RimeKey;
use crate::loader::Library;
use crate::{Error, api_fn, ffi};

/// Parameters for `RimeSetup` / `RimeInitialize`.
#[derive(Clone, Debug)]
pub struct Traits {
    pub shared_data_dir: PathBuf,
    pub user_data_dir: PathBuf,
    /// `None` lets librime pick a temporary directory; never pass an empty
    /// path, which makes glog write to stderr.
    pub log_dir: Option<PathBuf>,
    /// 0 = INFO, 1 = WARNING, 2 = ERROR, 3 = FATAL.
    pub min_log_level: i32,
    pub app_name: String,
    pub distribution_name: String,
    pub distribution_code_name: String,
    pub distribution_version: String,
    pub staging_dir: Option<PathBuf>,
    pub prebuilt_data_dir: Option<PathBuf>,
}

/// A message from `RimeSetNotificationHandler`, e.g. `("deploy", "success")`
/// or `("option", "!ascii_mode")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub session_id: usize,
    pub message_type: String,
    pub message_value: String,
}

pub type NotificationHandler = Box<dyn Fn(Notification) + Send + Sync>;

struct Inner {
    lib: Arc<Library>,
    // Kept alive for librime, which may retain the pointers from setup.
    _strings: Vec<CString>,
    handler: *mut NotificationHandler,
}

// SAFETY: `handler` is only dereferenced by librime's callback and freed in
// Drop after librime has been finalized.
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Inner {
    fn api(&self) -> &ffi::RimeApi {
        self.lib.api()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let api = self.api();
        unsafe {
            if let Some(f) = api_fn!(api, finalize) {
                f();
            }
            if let Some(f) = api_fn!(api, set_notification_handler) {
                f(None, std::ptr::null_mut());
            }
            if !self.handler.is_null() {
                drop(Box::from_raw(self.handler));
            }
        }
    }
}

/// An initialized librime runtime. Must exist at most once per process.
///
/// Sessions keep the runtime alive; `RimeFinalize` runs when the last of
/// `Rime` and its sessions is dropped.
#[derive(Clone)]
pub struct Rime {
    inner: Arc<Inner>,
}

unsafe extern "C" fn notification_trampoline(
    context_object: *mut c_void,
    session_id: ffi::RimeSessionId,
    message_type: *const c_char,
    message_value: *const c_char,
) {
    if context_object.is_null() {
        return;
    }
    let handler = unsafe { &*(context_object as *const NotificationHandler) };
    let n = Notification {
        session_id,
        message_type: unsafe { lossy(message_type) }.unwrap_or_default(),
        message_value: unsafe { lossy(message_value) }.unwrap_or_default(),
    };
    // Never unwind into C.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(n)));
}

impl Rime {
    /// Runs `setup`, installs the notification handler and `initialize`s
    /// librime. Deployment is left to the caller.
    pub fn init(
        lib: Arc<Library>,
        traits: &Traits,
        handler: Option<NotificationHandler>,
    ) -> Result<Self, Error> {
        let mut strings = Vec::new();
        let mut cstr = |s: &str| -> Result<*const c_char, Error> {
            let c = CString::new(s).map_err(|_| Error::InvalidString(s.to_owned()))?;
            let p = c.as_ptr();
            strings.push(c);
            Ok(p)
        };
        let path_str = |p: &Path| p.to_string_lossy().into_owned();

        let mut raw = ffi::RimeTraits {
            data_size: ffi::struct_data_size::<ffi::RimeTraits>(),
            shared_data_dir: cstr(&path_str(&traits.shared_data_dir))?,
            user_data_dir: cstr(&path_str(&traits.user_data_dir))?,
            distribution_name: cstr(&traits.distribution_name)?,
            distribution_code_name: cstr(&traits.distribution_code_name)?,
            distribution_version: cstr(&traits.distribution_version)?,
            app_name: cstr(&traits.app_name)?,
            modules: std::ptr::null(),
            min_log_level: traits.min_log_level as c_int,
            log_dir: match &traits.log_dir {
                Some(p) => cstr(&path_str(p))?,
                None => std::ptr::null(),
            },
            prebuilt_data_dir: match &traits.prebuilt_data_dir {
                Some(p) => cstr(&path_str(p))?,
                None => std::ptr::null(),
            },
            staging_dir: match &traits.staging_dir {
                Some(p) => cstr(&path_str(p))?,
                None => std::ptr::null(),
            },
        };

        let api = lib.api();
        let setup = api_fn!(api, setup).ok_or(Error::Missing("setup"))?;
        let initialize = api_fn!(api, initialize).ok_or(Error::Missing("initialize"))?;
        let set_handler = api_fn!(api, set_notification_handler)
            .ok_or(Error::Missing("set_notification_handler"))?;

        let handler = match handler {
            Some(h) => Box::into_raw(Box::new(h)),
            None => std::ptr::null_mut(),
        };
        unsafe {
            setup(&mut raw);
            if handler.is_null() {
                set_handler(None, std::ptr::null_mut());
            } else {
                set_handler(Some(notification_trampoline), handler as *mut c_void);
            }
            initialize(&mut raw);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                lib,
                _strings: strings,
                handler,
            }),
        })
    }

    fn api(&self) -> &ffi::RimeApi {
        self.inner.api()
    }

    pub fn library(&self) -> &Library {
        &self.inner.lib
    }

    /// `RimeStartMaintenance`. With `full_check == false` librime deploys
    /// only when its own modification check fires. Returns whether a
    /// maintenance thread was started.
    pub fn start_maintenance(&self, full_check: bool) -> bool {
        match api_fn!(self.api(), start_maintenance) {
            Some(f) => unsafe { f(full_check as c_int) != 0 },
            None => false,
        }
    }

    pub fn is_maintenance_mode(&self) -> bool {
        match api_fn!(self.api(), is_maintenance_mode) {
            Some(f) => unsafe { f() != 0 },
            None => false,
        }
    }

    pub fn join_maintenance_thread(&self) {
        if let Some(f) = api_fn!(self.api(), join_maintenance_thread) {
            unsafe { f() }
        }
    }

    /// Runs a deployer task synchronously, e.g. `installation_update`.
    pub fn run_task(&self, name: &str) -> bool {
        let Ok(name) = CString::new(name) else {
            return false;
        };
        match api_fn!(self.api(), run_task) {
            Some(f) => unsafe { f(name.as_ptr()) != 0 },
            None => false,
        }
    }

    /// `RimeSyncUserData`: destroys all sessions and starts an asynchronous
    /// maintenance run that syncs user dictionaries.
    pub fn sync_user_data(&self) -> bool {
        match api_fn!(self.api(), sync_user_data) {
            Some(f) => unsafe { f() != 0 },
            None => false,
        }
    }

    pub fn create_session(&self) -> Result<Session, Error> {
        let f = api_fn!(self.api(), create_session).ok_or(Error::Missing("create_session"))?;
        let id = unsafe { f() };
        if id == 0 {
            return Err(Error::Session);
        }
        Ok(Session {
            rime: self.clone(),
            id,
        })
    }

    pub fn schema_list(&self) -> Vec<SchemaListItem> {
        let (Some(get), Some(free)) = (
            api_fn!(self.api(), get_schema_list),
            api_fn!(self.api(), free_schema_list),
        ) else {
            return Vec::new();
        };
        let mut list = ffi::RimeSchemaList {
            size: 0,
            list: std::ptr::null_mut(),
        };
        unsafe {
            if get(&mut list) == 0 {
                return Vec::new();
            }
            let items = if list.list.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(list.list, list.size)
                    .iter()
                    .map(|it| SchemaListItem {
                        schema_id: lossy(it.schema_id).unwrap_or_default(),
                        name: lossy(it.name).unwrap_or_default(),
                    })
                    .collect()
            };
            free(&mut list);
            items
        }
    }

    /// Opens a compiled schema config, e.g. `luna_pinyin`.
    pub fn schema_config(&self, schema_id: &str) -> Option<Config> {
        self.open_config(api_fn!(self.api(), schema_open), schema_id)
    }

    /// Opens a compiled config, e.g. `default`.
    pub fn config(&self, config_id: &str) -> Option<Config> {
        self.open_config(api_fn!(self.api(), config_open), config_id)
    }

    /// Opens a user config such as `user` (`user.yaml`).
    pub fn user_config(&self, config_id: &str) -> Option<Config> {
        self.open_config(api_fn!(self.api(), user_config_open), config_id)
    }

    fn open_config(
        &self,
        open: Option<unsafe extern "C" fn(*const c_char, *mut ffi::RimeConfig) -> ffi::Bool>,
        id: &str,
    ) -> Option<Config> {
        let open = open?;
        let id = CString::new(id).ok()?;
        let mut raw = ffi::RimeConfig {
            ptr: std::ptr::null_mut(),
        };
        if unsafe { open(id.as_ptr(), &mut raw) } == 0 || raw.ptr.is_null() {
            return None;
        }
        Some(Config {
            rime: self.clone(),
            raw,
        })
    }

    pub fn shared_data_dir(&self) -> Option<String> {
        self.static_str(api_fn!(self.api(), get_shared_data_dir))
    }

    pub fn user_data_dir(&self) -> Option<String> {
        self.static_str(api_fn!(self.api(), get_user_data_dir))
    }

    pub fn sync_dir(&self) -> Option<String> {
        self.static_str(api_fn!(self.api(), get_sync_dir))
    }

    fn static_str(&self, f: Option<unsafe extern "C" fn() -> *const c_char>) -> Option<String> {
        unsafe { lossy(f?()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaListItem {
    pub schema_id: String,
    pub name: String,
}

/// A rime input session. Destroyed on drop.
pub struct Session {
    rime: Rime,
    id: ffi::RimeSessionId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composition {
    /// Byte offsets into `preedit`.
    pub length: usize,
    pub cursor_pos: usize,
    pub sel_start: usize,
    pub sel_end: usize,
    pub preedit: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    pub page_size: usize,
    pub page_no: usize,
    pub is_last_page: bool,
    pub highlighted: usize,
    pub candidates: Vec<Candidate>,
    pub select_keys: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub composition: Option<Composition>,
    pub menu: Menu,
    pub commit_text_preview: Option<String>,
    pub select_labels: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub schema_id: String,
    pub schema_name: String,
    pub is_disabled: bool,
    pub is_composing: bool,
    pub is_ascii_mode: bool,
    pub is_full_shape: bool,
    pub is_simplified: bool,
    pub is_traditional: bool,
    pub is_ascii_punct: bool,
}

impl Session {
    fn api(&self) -> &ffi::RimeApi {
        self.rime.api()
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Whether librime still knows this session; `sync_user_data` destroys
    /// all sessions behind the frontend's back.
    pub fn is_alive(&self) -> bool {
        match api_fn!(self.api(), find_session) {
            Some(f) => unsafe { f(self.id) != 0 },
            None => false,
        }
    }

    /// Returns whether rime consumed the key.
    pub fn process_key(&self, key: RimeKey) -> bool {
        match api_fn!(self.api(), process_key) {
            Some(f) => unsafe { f(self.id, key.keycode as c_int, key.mask as c_int) != 0 },
            None => false,
        }
    }

    pub fn commit_composition(&self) -> bool {
        match api_fn!(self.api(), commit_composition) {
            Some(f) => unsafe { f(self.id) != 0 },
            None => false,
        }
    }

    pub fn clear_composition(&self) {
        if let Some(f) = api_fn!(self.api(), clear_composition) {
            unsafe { f(self.id) }
        }
    }

    /// Takes the pending commit text, if any.
    pub fn take_commit(&self) -> Option<String> {
        let get = api_fn!(self.api(), get_commit)?;
        let free = api_fn!(self.api(), free_commit)?;
        let mut commit = ffi::RimeCommit {
            data_size: ffi::struct_data_size::<ffi::RimeCommit>(),
            text: std::ptr::null_mut(),
        };
        unsafe {
            if get(self.id, &mut commit) == 0 {
                return None;
            }
            let text = lossy(commit.text);
            free(&mut commit);
            text
        }
    }

    pub fn context(&self) -> Option<Context> {
        let get = api_fn!(self.api(), get_context)?;
        let free = api_fn!(self.api(), free_context)?;
        // SAFETY: all-zero is a valid RimeContext (null pointers, zero ints).
        let mut ctx: ffi::RimeContext = unsafe { std::mem::zeroed() };
        ctx.data_size = ffi::struct_data_size::<ffi::RimeContext>();
        unsafe {
            if get(self.id, &mut ctx) == 0 {
                return None;
            }
            let c = &ctx.composition;
            let composition = lossy(c.preedit).map(|preedit| Composition {
                length: c.length.max(0) as usize,
                cursor_pos: c.cursor_pos.max(0) as usize,
                sel_start: c.sel_start.max(0) as usize,
                sel_end: c.sel_end.max(0) as usize,
                preedit,
            });
            let m = &ctx.menu;
            let candidates = if m.candidates.is_null() || m.num_candidates <= 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(m.candidates, m.num_candidates as usize)
                    .iter()
                    .map(|cand| Candidate {
                        text: lossy(cand.text).unwrap_or_default(),
                        comment: lossy(cand.comment).filter(|s| !s.is_empty()),
                    })
                    .collect()
            };
            let menu = Menu {
                page_size: m.page_size.max(0) as usize,
                page_no: m.page_no.max(0) as usize,
                is_last_page: m.is_last_page != 0,
                highlighted: m.highlighted_candidate_index.max(0) as usize,
                candidates,
                select_keys: lossy(m.select_keys).filter(|s| !s.is_empty()),
            };
            let has = |offset| ffi::api_has_member(ctx.data_size, offset);
            let commit_text_preview =
                if has(std::mem::offset_of!(ffi::RimeContext, commit_text_preview)) {
                    lossy(ctx.commit_text_preview)
                } else {
                    None
                };
            let mut select_labels = Vec::new();
            if has(std::mem::offset_of!(ffi::RimeContext, select_labels))
                && !ctx.select_labels.is_null()
            {
                for i in 0..menu.page_size {
                    match lossy(*ctx.select_labels.add(i)) {
                        Some(l) => select_labels.push(l),
                        None => break,
                    }
                }
            }
            free(&mut ctx);
            Some(Context {
                composition,
                menu,
                commit_text_preview,
                select_labels,
            })
        }
    }

    pub fn status(&self) -> Option<Status> {
        let get = api_fn!(self.api(), get_status)?;
        let free = api_fn!(self.api(), free_status)?;
        let mut st: ffi::RimeStatus = unsafe { std::mem::zeroed() };
        st.data_size = ffi::struct_data_size::<ffi::RimeStatus>();
        unsafe {
            if get(self.id, &mut st) == 0 {
                return None;
            }
            let status = Status {
                schema_id: lossy(st.schema_id).unwrap_or_default(),
                schema_name: lossy(st.schema_name).unwrap_or_default(),
                is_disabled: st.is_disabled != 0,
                is_composing: st.is_composing != 0,
                is_ascii_mode: st.is_ascii_mode != 0,
                is_full_shape: st.is_full_shape != 0,
                is_simplified: st.is_simplified != 0,
                is_traditional: st.is_traditional != 0,
                is_ascii_punct: st.is_ascii_punct != 0,
            };
            free(&mut st);
            Some(status)
        }
    }

    pub fn set_option(&self, option: &str, value: bool) {
        let (Some(f), Ok(opt)) = (api_fn!(self.api(), set_option), CString::new(option)) else {
            return;
        };
        unsafe { f(self.id, opt.as_ptr(), value as c_int) }
    }

    pub fn get_option(&self, option: &str) -> bool {
        let (Some(f), Ok(opt)) = (api_fn!(self.api(), get_option), CString::new(option)) else {
            return false;
        };
        unsafe { f(self.id, opt.as_ptr()) != 0 }
    }

    pub fn current_schema(&self) -> Option<String> {
        let f = api_fn!(self.api(), get_current_schema)?;
        let mut buf = [0 as c_char; 256];
        unsafe {
            if f(self.id, buf.as_mut_ptr(), buf.len()) == 0 {
                return None;
            }
            lossy(buf.as_ptr())
        }
    }

    pub fn select_schema(&self, schema_id: &str) -> bool {
        let (Some(f), Ok(id)) = (api_fn!(self.api(), select_schema), CString::new(schema_id))
        else {
            return false;
        };
        unsafe { f(self.id, id.as_ptr()) != 0 }
    }

    /// The raw input string, e.g. `women'mingtian`.
    pub fn input(&self) -> Option<String> {
        let f = api_fn!(self.api(), get_input)?;
        unsafe { lossy(f(self.id)) }
    }

    pub fn caret_pos(&self) -> usize {
        match api_fn!(self.api(), get_caret_pos) {
            Some(f) => unsafe { f(self.id) },
            None => 0,
        }
    }

    pub fn select_candidate_on_current_page(&self, index: usize) -> bool {
        match api_fn!(self.api(), select_candidate_on_current_page) {
            Some(f) => unsafe { f(self.id, index) != 0 },
            None => false,
        }
    }

    /// `None` when the loaded librime predates `change_page`.
    pub fn change_page(&self, backward: bool) -> Option<bool> {
        let f = api_fn!(self.api(), change_page)?;
        Some(unsafe { f(self.id, backward as c_int) != 0 })
    }

    /// The label of an option state as declared in the schema's `switches`,
    /// e.g. `中` / `A` for `ascii_mode`.
    pub fn state_label(&self, option: &str, state: bool, abbreviated: bool) -> Option<String> {
        let opt = CString::new(option).ok()?;
        if let Some(f) = api_fn!(self.api(), get_state_label_abbreviated) {
            let slice = unsafe { f(self.id, opt.as_ptr(), state as c_int, abbreviated as c_int) };
            if slice.str_.is_null() {
                return None;
            }
            let bytes =
                unsafe { std::slice::from_raw_parts(slice.str_ as *const u8, slice.length) };
            return Some(String::from_utf8_lossy(bytes).into_owned()).filter(|s| !s.is_empty());
        }
        let f = api_fn!(self.api(), get_state_label)?;
        unsafe { lossy(f(self.id, opt.as_ptr(), state as c_int)) }.filter(|s| !s.is_empty())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(f) = api_fn!(self.api(), destroy_session) {
            unsafe {
                f(self.id);
            }
        }
    }
}

/// An open rime config (schema, `default`, `user`, ...). Closed on drop.
pub struct Config {
    rime: Rime,
    raw: ffi::RimeConfig,
}

impl Config {
    fn api(&self) -> &ffi::RimeApi {
        self.rime.api()
    }

    pub fn get_bool(&mut self, key: &str) -> Option<bool> {
        let f = api_fn!(self.api(), config_get_bool)?;
        let key = CString::new(key).ok()?;
        let mut v: ffi::Bool = 0;
        (unsafe { f(&mut self.raw, key.as_ptr(), &mut v) } != 0).then_some(v != 0)
    }

    pub fn get_int(&mut self, key: &str) -> Option<i32> {
        let f = api_fn!(self.api(), config_get_int)?;
        let key = CString::new(key).ok()?;
        let mut v: c_int = 0;
        (unsafe { f(&mut self.raw, key.as_ptr(), &mut v) } != 0).then_some(v)
    }

    pub fn get_string(&mut self, key: &str) -> Option<String> {
        let f = api_fn!(self.api(), config_get_cstring)?;
        let key = CString::new(key).ok()?;
        unsafe { lossy(f(&mut self.raw, key.as_ptr())) }
    }

    pub fn list_size(&mut self, key: &str) -> usize {
        let (Some(f), Ok(key)) = (api_fn!(self.api(), config_list_size), CString::new(key)) else {
            return 0;
        };
        unsafe { f(&mut self.raw, key.as_ptr()) }
    }

    /// Keys of the map at `key`, in document order.
    pub fn map_keys(&mut self, key: &str) -> Vec<String> {
        let (Some(begin), Some(next), Some(end), Ok(key)) = (
            api_fn!(self.api(), config_begin_map),
            api_fn!(self.api(), config_next),
            api_fn!(self.api(), config_end),
            CString::new(key),
        ) else {
            return Vec::new();
        };
        let mut it: ffi::RimeConfigIterator = unsafe { std::mem::zeroed() };
        let mut keys = Vec::new();
        unsafe {
            if begin(&mut it, &mut self.raw, key.as_ptr()) == 0 {
                return keys;
            }
            while next(&mut it) != 0 {
                if let Some(k) = lossy(it.key) {
                    keys.push(k);
                }
            }
            end(&mut it);
        }
        keys
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        if let Some(f) = api_fn!(self.api(), config_close) {
            unsafe {
                f(&mut self.raw);
            }
        }
    }
}

/// Copies a possibly-null C string.
unsafe fn lossy(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}
