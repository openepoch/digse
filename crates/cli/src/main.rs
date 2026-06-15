//! Digse CLI application
//!
//! Subcommand layout:
//!   digse start [options]   -- start the local HTTP server (background daemon)
//!   digse restart [options] -- restart the running server (or start it)
//!   digse stop              -- stop the running server
//!   digse ps                -- report whether the server is running
//!   digse startup status|add|remove
//!                          -- manage boot-time autostart (systemd user service)
//!   digse list engines|categories
//!   digse config <sub>      -- view/change persisted config (~/.digse/config.toml)
//!
//! Searching happens via the server's GET /search endpoint — or the HTML UI at /.

use clap::{Parser, Subcommand};

mod daemon;
mod filters;
mod pidfile;
mod start;
mod startup;

/// Digse - Dig Search Engines - A lightweight metasearch engine CLI
#[derive(Parser, Debug)]
#[command(name = "digse")]
#[command(author = "digse contributors")]
#[command(version = digse::VERSION)]
#[command(about = "Dig Search Engines - Lightweight metasearch engine CLI", long_about = None)]
struct Cli {
    /// Verbosity level (-v, -vv, -vvv, -vvvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Quiet mode (minimal output)
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the digse server as a background daemon.
    Start(StartArgs),

    /// Restart the running server (or start it if stopped).
    Restart(StartArgs),

    /// Stop the running server.
    Stop,

    /// Report whether the digse server is running.
    Ps,

    /// Print the digse version and build target (the `--version` flag is the terse form).
    Version,

    /// Manage boot-time autostart (systemd user service on Linux, registry Run key on Windows).
    #[command(subcommand)]
    Startup(StartupCommand),

    /// View or change persisted digse configuration (~/.digse/config.toml).
    #[command(subcommand)]
    Config(ConfigCommand),

    /// List available engines or engine categories.
    #[command(subcommand)]
    List(ListCommand),

    /// INTERNAL: foreground server entry point for the daemon re-exec.
    #[command(name = "__start_foreground__", hide = true)]
    #[allow(non_camel_case_types)]
    __start_foreground__(StartArgs),
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    /// Print the path to the config file.
    Path,

    /// Print the full resolved configuration as TOML.
    Show,

    /// Write the current (default) configuration to disk if no file exists.
    Init,

    /// Get a single config value by dotted key (e.g. `serve.port`).
    Get {
        /// Dotted config key, e.g. `serve.port` or `search.count`.
        key: String,
    },

    /// Set a single config value by dotted key and persist it.
    Set {
        /// Dotted config key, e.g. `serve.host` or `search.timeout_seconds`.
        key: String,
        /// New value for the key.
        value: String,
    },
}

#[derive(Subcommand, Debug)]
enum StartupCommand {
    /// Show whether autostart is enabled and the service active.
    Status,

    /// Install + enable the autostart user service (starts at boot).
    Add,

    /// Disable + remove the autostart user service.
    Remove,
}

#[derive(Subcommand, Debug)]
enum ListCommand {
    /// List search engines.
    Engines {
        /// Include disabled engines as well.
        #[arg(long)]
        all: bool,
    },
    /// List engine categories.
    Categories,
}

#[derive(Parser, Debug)]
struct StartArgs {
    /// Host/interface to bind (overrides config `serve.host`)
    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    /// Port to listen on (overrides config `serve.port`)
    #[arg(long, value_name = "PORT")]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Setup logging based on verbosity (applies to every subcommand).
    setup_logging(cli.verbose, cli.quiet);

    match cli.command {
        Command::Start(args) => run_start(args),
        Command::Restart(args) => run_restart(args),
        Command::Stop => run_stop(),
        Command::Ps => run_ps(),
        Command::Version => run_version(),
        Command::Startup(cmd) => {
            match cmd {
                StartupCommand::Status => startup::status(),
                StartupCommand::Add => startup::add(),
                StartupCommand::Remove => startup::remove(),
            }?;
            Ok(())
        }
        Command::Config(config_cmd) => {
            run_config(config_cmd)?;
            Ok(())
        }
        Command::List(list_cmd) => {
            match list_cmd {
                ListCommand::Engines { all } => list_engines(all),
                ListCommand::Categories => list_categories(),
            }
            Ok(())
        }
        Command::__start_foreground__(args) => run_start_foreground(args).await,
    }
}

/// Load the persisted config, falling back to defaults with a warning on error.
fn load_config_or_warn() -> digse::DigseConfig {
    match digse::DigseConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Warning: could not load config (using defaults): {}", e);
            digse::DigseConfig::default()
        }
    }
}

/// `digse start` — spawn the server as a detached background daemon.
fn run_start(args: StartArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config_or_warn();
    let host = args.host.unwrap_or_else(|| cfg.serve.host.clone());
    let port = args.port.unwrap_or(cfg.serve.port);

    if let pidfile::Liveness::Alive(rec) = pidfile::probe()? {
        return Err(format!(
            "digse start: already running (pid {}) on http://{}:{} — use 'digse restart' to restart",
            rec.pid, rec.host, rec.port
        )
        .into());
    }

    let pid = daemon::start_server_background(&host, port)?;
    println!("digse start: started (pid {}) on http://{}:{}", pid, host, port);
    Ok(())
}

/// `digse restart` — stop (if running) then start fresh.
fn run_restart(args: StartArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config_or_warn();
    let host = args.host.unwrap_or_else(|| cfg.serve.host.clone());
    let port = args.port.unwrap_or(cfg.serve.port);

    let was_running = daemon::stop_server()?;
    let pid = daemon::start_server_background(&host, port)?;
    if was_running {
        println!("digse start: restarted (pid {}) on http://{}:{}", pid, host, port);
    } else {
        println!(
            "digse start: was not running; started (pid {}) on http://{}:{}",
            pid, host, port
        );
    }
    Ok(())
}

/// `digse stop` — stop the running server (idempotent: exits 0 even if not running).
fn run_stop() -> Result<(), Box<dyn std::error::Error>> {
    match daemon::stop_server()? {
        true => println!("digse start: stopped"),
        false => println!("digse start: not running"),
    }
    Ok(())
}

/// `digse ps` — report server liveness. Exit 0 when running, 1 when stopped.
fn run_ps() -> Result<(), Box<dyn std::error::Error>> {
    match pidfile::probe()? {
        pidfile::Liveness::Alive(rec) => {
            println!("digse start: running");
            println!("pid: {}", rec.pid);
            println!("url: http://{}:{}", rec.host, rec.port);
            Ok(())
        }
        pidfile::Liveness::Stopped => {
            println!("digse start: stopped");
            std::process::exit(1);
        }
    }
}

/// `digse version` — print version + build target. The terse `--version` flag
/// (from clap's `#[command(version)]`) prints just `digse <ver>`; this adds the
/// OS/arch triple so install/update scripts and humans can see the platform.
fn run_version() -> Result<(), Box<dyn std::error::Error>> {
    println!("digse {}", digse::VERSION);
    println!("target: {}-{}", std::env::consts::OS, std::env::consts::ARCH);
    Ok(())
}

/// Internal: the daemon child's actual entry point — bind + serve in the
/// foreground (its parent detached it; stdio is redirected to the log file).
async fn run_start_foreground(args: StartArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config_or_warn();
    let host = args.host.unwrap_or_else(|| cfg.serve.host.clone());
    let port = args.port.unwrap_or(cfg.serve.port);
    start::run_foreground(&host, port).await?;
    Ok(())
}

/// `digse config <sub>` — view or change the persisted configuration.
fn run_config(cmd: ConfigCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ConfigCommand::Path => {
            let path = digse::DigseConfig::config_path()?;
            println!("{}", path.display());
        }
        ConfigCommand::Show => {
            let cfg = digse::DigseConfig::load().unwrap_or_default();
            println!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigCommand::Init => {
            let path = digse::DigseConfig::config_path()?;
            if path.exists() {
                eprintln!("Config already exists at {}", path.display());
            } else {
                let cfg = digse::DigseConfig::default();
                let written = cfg.save()?;
                eprintln!("Wrote default config to {}", written.display());
            }
            println!("{}", path.display());
        }
        ConfigCommand::Get { key } => {
            let cfg = digse::DigseConfig::load().unwrap_or_default();
            match cfg.get(&key) {
                Some(value) => println!("{}", value),
                None => return Err(format!("unknown config key '{}'", key).into()),
            }
        }
        ConfigCommand::Set { key, value } => {
            let mut cfg = digse::DigseConfig::load().unwrap_or_default();
            cfg.set(&key, &value)?;
            let path = cfg.save()?;
            eprintln!("Set {} = {}  (saved to {})", key, value, path.display());
        }
    }
    Ok(())
}

fn list_engines(list_all: bool) {
    use digse_engines::all_engines;

    let engines = all_engines();

    println!("Available engines ({}):", engines.len());

    for engine in engines {
        if !list_all && !engine.is_enabled() {
            continue;
        }

        let status = if engine.is_enabled() { "✓" } else { "✗" };
        println!(
            "  {} {:<24} {}",
            status,
            engine.name(),
            engine.category().as_str()
        );

        let desc = engine.metadata().description;
        if !desc.is_empty() {
            println!("      {}", desc);
        }
    }
}

fn list_categories() {
    println!("Available engine categories:");

    let categories = vec![
        ("general", "General web search"),
        ("images", "Image search"),
        ("videos", "Video search"),
        ("music", "Music/audio search"),
        ("news", "News search"),
        ("science", "Academic/scientific search"),
        ("it", "IT/programming search"),
        ("files", "File/document search"),
        ("social", "Social media search"),
        ("maps", "Maps/location search"),
        ("shopping", "Shopping/e-commerce"),
        ("weather", "Weather search"),
    ];

    for (name, description) in categories {
        println!("  {} - {}", name, description);
    }
}

fn setup_logging(verbose: u8, quiet: bool) {
    let log_level = match verbose {
        0 if quiet => log::LevelFilter::Warn,
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    env_logger::Builder::new()
        .filter_level(log_level)
        .init();
}
