mod app;
mod buffer;
mod clipboard;
mod config;
mod edit;
mod engine;
mod history;
mod instance;
mod keys;
mod paths;
mod theme;
mod tty;
mod ui;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event};
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};
use rime_dl::{Library, Notification, Rime, Traits};

use crate::app::{App, Effect, Exit, Level, Mode};
use crate::clipboard::Clipboard;
use crate::config::{Config, DeployOnStartup, Keymap, ThemeMode, Tristate};
use crate::edit::EditFile;
use crate::engine::{ImeEngine, RimeEngine};
use crate::history::History;
use crate::instance::Instance;
use crate::theme::Theme;
use crate::ui::UiContext;

#[derive(Parser)]
#[command(
    version,
    about = "端砚：基于 rime 的终端中文输入草稿板",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Edit FILE and write it back on submit (for use as $EDITOR).
    #[arg(value_name = "FILE", conflicts_with_all = ["stdout", "print_default_config"])]
    file: Option<PathBuf>,
    /// Config file (default: ~/.config/duanyan/config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Print the submitted text to stdout and exit (compact inline UI).
    #[arg(long)]
    stdout: bool,
    /// With --stdout: use the fullscreen layout.
    #[arg(long, requires = "stdout")]
    fullscreen: bool,
    /// Print the default config and exit.
    #[arg(long)]
    print_default_config: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Deploy the rime configuration.
    Deploy {
        /// Rebuild even if nothing appears to have changed.
        #[arg(long)]
        full: bool,
    },
    /// Sync the user dictionaries (rime's sync_dir).
    Sync,
    /// Show the resolved librime and data directories.
    Info,
    /// Print the shell integration script: `^^` then Tab opens duanyan.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    fn script(self) -> &'static str {
        match self {
            Shell::Zsh => include_str!("shell/duanyan.zsh"),
            Shell::Bash => include_str!("shell/duanyan.bash"),
            Shell::Fish => include_str!("shell/duanyan.fish"),
        }
    }
}

/// Everything resolved from config and environment before touching rime.
struct Setup {
    config: Config,
    config_path: PathBuf,
    user_data_dir: PathBuf,
    shared_data_dir: PathBuf,
    /// `None` when no shared data dir was found.
    shared_source: Option<paths::SharedDirSource>,
    state_dir: PathBuf,
    log_dir: PathBuf,
}

impl Setup {
    fn resolve(cli: &Cli) -> anyhow::Result<Self> {
        let config_path = cli
            .config
            .clone()
            .unwrap_or_else(paths::default_config_file);
        let config = Config::load(&config_path)?;
        let user_data_dir = config
            .rime
            .user_data_dir
            .as_deref()
            .map(paths::expand_tilde)
            .unwrap_or_else(paths::default_user_data_dir);
        let shared =
            paths::find_shared_data_dir(config.rime.shared_data_dir.as_deref(), &paths::RealEnv);
        let state_dir = paths::state_dir();
        let (shared_data_dir, shared_source) = match shared {
            Some((dir, source)) => (dir, Some(source)),
            None => (user_data_dir.clone(), None),
        };
        Ok(Self {
            shared_data_dir,
            shared_source,
            log_dir: state_dir.join("log"),
            config,
            config_path,
            user_data_dir,
            state_dir,
        })
    }

    fn load_library(&self) -> anyhow::Result<Library> {
        let candidates =
            paths::librime_candidates(self.config.rime.librime_path.as_deref(), &paths::RealEnv);
        Library::open_first(&candidates).map_err(|errors| {
            let mut msg = String::from("could not load librime; tried:\n");
            for e in errors {
                msg.push_str(&format!("  - {e}\n"));
            }
            msg.push_str("set rime.librime_path in the config or DUANYAN_LIBRIME_PATH");
            anyhow::anyhow!(msg)
        })
    }

    fn init_rime(
        &self,
        lib: Library,
        handler: impl Fn(Notification) + Send + Sync + 'static,
    ) -> anyhow::Result<Rime> {
        std::fs::create_dir_all(&self.user_data_dir)
            .with_context(|| format!("creating {}", self.user_data_dir.display()))?;
        // glog does not create its log directory; without it every log line
        // is reported on stderr.
        std::fs::create_dir_all(&self.log_dir)
            .with_context(|| format!("creating {}", self.log_dir.display()))?;
        let rime = Rime::init(
            Arc::new(lib),
            &Traits {
                shared_data_dir: self.shared_data_dir.clone(),
                user_data_dir: self.user_data_dir.clone(),
                log_dir: Some(self.log_dir.clone()),
                min_log_level: self.config.rime.log_level.glog(),
                app_name: "rime.duanyan".into(),
                distribution_name: "Duanyan".into(),
                distribution_code_name: "duanyan".into(),
                distribution_version: env!("CARGO_PKG_VERSION").into(),
                staging_dir: None,
                prebuilt_data_dir: None,
            },
            Some(Box::new(handler)),
        )?;
        Ok(rime)
    }

    fn data_dirs(&self) -> Vec<&Path> {
        if self.shared_data_dir == self.user_data_dir {
            vec![&self.user_data_dir]
        } else {
            vec![&self.user_data_dir, &self.shared_data_dir]
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.print_default_config {
        print!("{}", config::DEFAULT_CONFIG);
        return ExitCode::SUCCESS;
    }
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("duanyan: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    if let Some(Command::Init { shell }) = cli.command {
        print!("{}", shell.script());
        return Ok(ExitCode::SUCCESS);
    }
    let setup = Setup::resolve(&cli)?;
    match cli.command {
        Some(Command::Info) => info(&setup),
        Some(Command::Deploy { full }) => maintenance(&setup, Some(full)),
        Some(Command::Sync) => maintenance(&setup, None),
        Some(Command::Init { .. }) => unreachable!(),
        None => {
            let (mode, edit) = match cli.file {
                Some(path) => (Mode::Edit, Some(EditFile::load(path)?)),
                None if cli.stdout => (Mode::Stdout, None),
                None => (Mode::Scratch, None),
            };
            tui(&setup, mode, cli.fullscreen, edit.as_ref())
        }
    }
}

fn info(setup: &Setup) -> anyhow::Result<ExitCode> {
    println!("config file      {}", setup.config_path.display());
    println!("user_data_dir    {}", setup.user_data_dir.display());
    println!(
        "shared_data_dir  {}{}",
        setup.shared_data_dir.display(),
        match setup.shared_source {
            Some(paths::SharedDirSource::Found) => "",
            Some(paths::SharedDirSource::Bundled) => " (bundled opencc data)",
            None => " (none found; using user_data_dir)",
        }
    );
    println!("state dir        {}", setup.state_dir.display());
    match setup.load_library() {
        Ok(lib) => println!(
            "librime          {} ({})",
            lib.path().display(),
            lib.version().unwrap_or_default()
        ),
        Err(e) => println!("librime          {e}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// `deploy` (`Some(full)`) or `sync` (`None`) without the TUI.
fn maintenance(setup: &Setup, deploy: Option<bool>) -> anyhow::Result<ExitCode> {
    let instance = Instance::acquire(&setup.state_dir.join("instance.lock"))?;
    anyhow::ensure!(
        instance.is_primary(),
        "another duanyan instance is running; close it first"
    );
    let lib = setup.load_library()?;
    let (tx, rx) = mpsc::channel();
    let rime = setup.init_rime(lib, move |n| {
        let _ = tx.send(n);
    })?;
    let started = match deploy {
        Some(full) => {
            let last = engine::last_build_time(&rime);
            if !full
                && !engine::is_first_run(&setup.user_data_dir)
                && !engine::needs_deploy(&setup.data_dirs(), last)
            {
                eprintln!("nothing changed since the last deploy (use --full to force)");
                return Ok(ExitCode::SUCCESS);
            }
            eprintln!("deploying {} ...", setup.user_data_dir.display());
            rime.start_maintenance(true)
        }
        None => {
            eprintln!("syncing user data ...");
            rime.sync_user_data()
        }
    };
    anyhow::ensure!(started, "librime did not start the maintenance task");
    rime.join_maintenance_thread();
    let failed = rx
        .try_iter()
        .any(|n| n.message_type == "deploy" && n.message_value == "failure");
    if failed {
        eprintln!("failed; see the logs in {}", setup.log_dir.display());
        Ok(ExitCode::FAILURE)
    } else {
        eprintln!("done");
        Ok(ExitCode::SUCCESS)
    }
}

fn tui(
    setup: &Setup,
    mode: Mode,
    fullscreen: bool,
    edit: Option<&EditFile>,
) -> anyhow::Result<ExitCode> {
    let cfg = &setup.config;
    // Catch config mistakes before touching the terminal; legacy-terminal
    // conflicts are checked again once KKP support is known.
    if let Err(errors) = Keymap::build(&cfg.keybinding, true) {
        anyhow::bail!("invalid keybindings:\n  {}", errors.join("\n  "));
    }
    let clipboard = Clipboard::from_config(&cfg.clipboard)?;
    Theme::MOCHA.with_overrides(&cfg.theme.colors)?;

    let instance = Instance::acquire(&setup.state_dir.join("instance.lock"))?;
    // glog copies ERROR logs to stderr regardless of log_dir; its flag
    // defaults come from GLOG_* variables read when librime is loaded.
    if std::env::var_os("GLOG_stderrthreshold").is_none() {
        // SAFETY: still single-threaded; librime is not loaded yet.
        unsafe { std::env::set_var("GLOG_stderrthreshold", "3") };
    }
    std::fs::create_dir_all(&setup.log_dir)?;
    let stderr_redirect = tty::StderrRedirect::to_file(&setup.log_dir.join("stderr.log"))?;
    let lib = setup.load_library()?;
    let lib_desc = format!(
        "{} ({})",
        lib.path().display(),
        lib.version().unwrap_or_default()
    );
    let (tx, rx) = mpsc::channel();
    let rime = setup.init_rime(lib, move |n| {
        let _ = tx.send(n);
    })?;
    let mut engine = RimeEngine::new(rime, rx);

    let history = if cfg.history.persist {
        let path = cfg
            .history
            .path
            .as_deref()
            .map(paths::expand_tilde)
            .unwrap_or_else(|| setup.state_dir.join("history.jsonl"));
        History::load(path, cfg.history.max_entries)?
    } else {
        History::ephemeral(cfg.history.max_entries)
    };

    // Startup deploy policy.
    let first_run = engine::is_first_run(&setup.user_data_dir);
    let mut deploy_hint = false;
    let mut start_deploy = false;
    if first_run {
        start_deploy = instance.is_primary();
    } else {
        match cfg.rime.deploy_on_startup {
            DeployOnStartup::Never => {}
            policy => {
                if engine::needs_deploy(&setup.data_dirs(), engine.last_build_time()) {
                    if policy == DeployOnStartup::Auto && instance.is_primary() {
                        start_deploy = true;
                    } else {
                        deploy_hint = true;
                    }
                }
            }
        }
    }
    if !start_deploy {
        engine.open_session()?;
    }
    let page_size = engine.page_size();

    // Terminal.
    let inline = mode == Mode::Stdout && !fullscreen;
    crossterm::terminal::enable_raw_mode()?;
    let mut tty = tty::open_tty()?;
    let want_probe = cfg.tui.kitty_keyboard == Tristate::Auto || cfg.theme.mode == ThemeMode::Auto;
    let probe = if want_probe {
        tty::probe(&mut tty, Duration::from_millis(300))
    } else {
        tty::Probe::default()
    };
    let kkp = match cfg.tui.kitty_keyboard {
        Tristate::On => true,
        Tristate::Off => false,
        Tristate::Auto => probe.kkp,
    };
    let keymap = match Keymap::build(&cfg.keybinding, kkp) {
        Ok(k) => k,
        Err(errors) => {
            let _ = crossterm::terminal::disable_raw_mode();
            anyhow::bail!(
                "invalid keybindings for a terminal without the kitty keyboard protocol:\n  {}",
                errors.join("\n  ")
            );
        }
    };
    let light = match cfg.theme.mode {
        ThemeMode::Dark => false,
        ThemeMode::Light => true,
        ThemeMode::Auto => probe
            .background
            .as_deref()
            .and_then(theme::is_light_background)
            .unwrap_or(false),
    };
    let base = if light { Theme::LATTE } else { Theme::MOCHA };
    let theme = base.with_overrides(&cfg.theme.colors)?;

    let modes = tty::Modes {
        alternate_screen: !inline,
        mouse: cfg.tui.mouse,
        kkp,
    };
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tty::restore(modes);
        default_hook(info);
    }));
    tty::enter(&mut tty, modes)?;

    let ctx = UiContext {
        inline,
        theme,
        candidate_layout: cfg.tui.candidate_layout,
        show_comment: cfg.tui.show_candidate_comment,
        edit_path: edit.map(|e| e.path.display().to_string()),
        info: vec![
            ("librime".into(), lib_desc),
            (
                "键盘协议".into(),
                if kkp {
                    "kitty keyboard protocol 已启用".into()
                } else {
                    "传统编码（kitty keyboard protocol 未启用）".into()
                },
            ),
            ("配置文件".into(), setup.config_path.display().to_string()),
            (
                "用户数据目录".into(),
                setup.user_data_dir.display().to_string(),
            ),
            (
                "共享数据目录".into(),
                match setup.shared_source {
                    Some(paths::SharedDirSource::Bundled) => format!(
                        "{}（bundled 包自带的 opencc 数据）",
                        setup.shared_data_dir.display()
                    ),
                    _ => setup.shared_data_dir.display().to_string(),
                },
            ),
            ("日志目录".into(), setup.log_dir.display().to_string()),
            (
                "用户词典".into(),
                if instance.is_primary() {
                    "学习新词"
                } else {
                    "已有端砚在运行，不学习新词"
                }
                .into(),
            ),
        ],
    };
    // ratatui's Viewport::Inline asks crossterm for the cursor position,
    // which queries through stdout; reserve the rows by hand instead.
    let inline_area = if inline {
        Some(tty::reserve_inline(
            &mut tty,
            ui::inline_height(&ctx, page_size),
        )?)
    } else {
        None
    };
    let viewport = match inline_area {
        Some(inline) => Viewport::Fixed(inline.area),
        None => Viewport::Fullscreen,
    };
    let backend = CrosstermBackend::new(tty.try_clone()?);
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport })?;

    let mut app = App::new(engine, keymap, history);
    app.mode = mode;
    if let Some(edit) = edit {
        app.open_file(&edit.original);
    }
    app.copy_on_submit = cfg.general.copy_on_submit;
    app.deploy_hint = deploy_hint;
    app.secondary = !instance.is_primary();
    if start_deploy {
        app.engine.start_deploy();
        app.notify(Level::Info, "首次运行，正在部署 rime…");
    } else if first_run {
        app.notify(Level::Error, "rime 尚未部署，请关闭其它端砚实例后重新启动");
    }

    let result = event_loop(&mut terminal, &mut app, &ctx, &clipboard, edit, &mut tty);

    drop(terminal);
    if let Some(inline) = &inline_area {
        let _ = tty::clear_inline(&mut tty, inline);
    }
    tty::restore(modes);
    let _ = std::panic::take_hook();
    // rime may flush its user dictionary and log while shutting down.
    let exit = app.exit.take();
    drop(app);
    drop(stderr_redirect);
    result?;

    Ok(match exit {
        Some(Exit::Submit(text)) => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())?;
            out.flush()?;
            ExitCode::SUCCESS
        }
        Some(Exit::Saved) => ExitCode::SUCCESS,
        Some(Exit::Cancel) => ExitCode::FAILURE,
        Some(Exit::Quit) | None => ExitCode::SUCCESS,
    })
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::fs::File>>,
    app: &mut App<RimeEngine>,
    ctx: &UiContext,
    clipboard: &Clipboard,
    edit: Option<&EditFile>,
    tty: &mut std::fs::File,
) -> anyhow::Result<()> {
    loop {
        let mut hits = Default::default();
        terminal.draw(|f| hits = ui::draw(f, app, ctx))?;
        app.hits = hits;
        if app.exit.is_some() {
            return Ok(());
        }
        let timeout = if app.engine.busy().is_some() { 50 } else { 200 };
        if event::poll(Duration::from_millis(timeout))? {
            loop {
                match event::read()? {
                    Event::Key(k) => app.handle_key(k),
                    Event::Paste(s) => app.paste(&s),
                    Event::Mouse(m) => app.mouse(m),
                    _ => {}
                }
                if app.exit.is_some() || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        app.tick();
        for effect in std::mem::take(&mut app.effects) {
            match effect {
                Effect::Copy(text) => {
                    if let Err(e) = clipboard.copy(&text, tty) {
                        app.notify(Level::Error, format!("复制失败：{e}"));
                    }
                }
                // Saved from inside the UI so that a failure leaves the
                // text on screen for another try.
                Effect::Save(text) => match edit.map(|e| e.save(&text)) {
                    Some(Ok(())) => app.exit = Some(Exit::Saved),
                    Some(Err(e)) => app.notify(Level::Error, format!("保存失败：{e}")),
                    None => {}
                },
            }
        }
    }
}
