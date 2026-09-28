use std::io::{IsTerminal, Write};
use std::sync::Arc;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};

use text_adventure_terminal::config::{self, ENV_TOKEN, ENV_URL};
use text_adventure_terminal::engine::http::HttpEngine;
use text_adventure_terminal::engine::{Engine, Reconnect};
use text_adventure_terminal::protocol::Screen;
use text_adventure_terminal::tui::start::latest_unfinished;
use text_adventure_terminal::{line, present, tui, upgrade};

/// Play text-adventure games from a terminal.
#[derive(Parser)]
#[command(name = "ta", version, about)]
struct Cli {
    /// The engine's address, as its operator gave it to you
    /// (overrides TA_ENGINE_URL and the config file).
    #[arg(long, global = true, value_name = "URL")]
    engine: Option<String>,
    /// Your player token (overrides TA_TOKEN and the stored token). Prefer
    /// `ta login`: a flag can be seen by other users of this machine.
    #[arg(long, global = true, value_name = "TOKEN", hide = true)]
    token: Option<String>,
    /// Skip the start screen and continue the game you started last.
    #[arg(long)]
    resume: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Play in the full-screen client (the default). Without a game or a
    /// world, it opens on a start screen listing both.
    Play {
        #[command(flatten)]
        game: GameArgs,
        /// Skip the start screen and continue the game you started last.
        #[arg(long, conflicts_with_all = ["game", "world"])]
        resume: bool,
    },
    /// Play in plain line mode: lines from stdin, prose and outcomes to stdout.
    /// Without a game or a world, it continues the game you started last.
    Line {
        #[command(flatten)]
        game: GameArgs,
        /// Write each finished turn as a line of JSON.
        #[arg(long)]
        json: bool,
    },
    /// List your games.
    Games,
    /// List the worlds a game can be started in.
    Worlds,
    /// Start a new game in a world.
    New { world: String },
    /// Show the engine, your player name and this month's spend.
    Whoami,
    /// Save the engine address and store your token (read from stdin).
    Login,
    /// Forget the stored token.
    Logout,
    /// Show where settings come from. Never prints the token.
    Config,
    /// Install the newest release, using your `gh` login.
    Upgrade {
        /// Only report whether a newer release exists.
        #[arg(long)]
        check: bool,
    },
}

#[derive(clap::Args, Default)]
struct GameArgs {
    /// The game to play.
    game: Option<String>,
    /// Start a new game in this world instead.
    #[arg(long, conflicts_with = "game")]
    world: Option<String>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn file_config() -> anyhow::Result<config::FileConfig> {
    match config::config_path() {
        Some(path) => config::load_file(&path),
        None => Ok(Default::default()),
    }
}

fn connect(cli: &Cli) -> anyhow::Result<HttpEngine> {
    let file = file_config()?;
    let url = config::resolve_url(cli.engine.as_deref(), env(ENV_URL).as_deref(), &file).context(
        "no engine address: run `ta login --engine <URL>`, set TA_ENGINE_URL, or pass --engine",
    )?;
    let token = config::resolve_token(cli.token.as_deref(), env(ENV_TOKEN).as_deref())
        .context("no player token: run `ta login`, or set TA_TOKEN")?;
    HttpEngine::new(&url, &token).map_err(anyhow::Error::msg)
}

fn engine_error(e: text_adventure_terminal::engine::EngineError) -> anyhow::Error {
    let (message, hint) = present::error_lines(&e);
    match hint {
        Some(h) => anyhow::anyhow!("{message}\n{h}"),
        None => anyhow::anyhow!(message),
    }
}

fn open_game(engine: &dyn Engine, args: &GameArgs) -> anyhow::Result<Screen> {
    if let Some(world) = &args.world {
        return engine.start_game(world).map_err(engine_error);
    }
    if let Some(game) = &args.game {
        return engine.screen(game).map_err(engine_error);
    }
    let games = engine.games().map_err(engine_error)?;
    if let Some(game) = latest_unfinished(&games) {
        return engine.screen(&game.id).map_err(engine_error);
    }
    let worlds = engine.worlds().map_err(engine_error)?;
    match worlds.as_slice() {
        [only] => engine.start_game(&only.id).map_err(engine_error),
        [] => bail!("the engine offers no worlds to play"),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|w| format!("  {}  {}", w.id, w.title))
                .collect();
            bail!(
                "you have no unfinished game. Start one with `ta new <world>`:\n{}",
                list.join("\n")
            )
        }
    }
}

/// What `ta play` opens on: the game or world named, else the start screen
/// or, with `--resume`, the game started last.
fn opening(
    engine: &dyn Engine,
    args: &GameArgs,
    resume: bool,
    label: &str,
) -> anyhow::Result<tui::Opening> {
    if args.game.is_some() || args.world.is_some() {
        return Ok(tui::Opening::Game(Box::new(open_game(engine, args)?)));
    }
    tui::opening(engine, resume, label).map_err(engine_error)
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("ta: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    if cli.resume && cli.command.is_some() {
        bail!("--resume goes on its own: `ta --resume`, or `ta play --resume`");
    }
    let default = Cmd::Play {
        game: GameArgs::default(),
        resume: cli.resume,
    };
    match cli.command.as_ref().unwrap_or(&default) {
        Cmd::Play { game, resume } => {
            if !std::io::stdout().is_terminal() {
                bail!("the full-screen client needs a terminal; use `ta line` for scripts");
            }
            let engine: Arc<dyn Engine> = Arc::new(connect(&cli)?);
            let label = match engine.service() {
                Ok(s) => format!("{} · {} {}", s.player.name, s.engine.name, s.engine.version),
                Err(e) => return Err(engine_error(e)),
            };
            let opening = opening(engine.as_ref(), game, *resume, &label)?;
            tui::run(engine, opening, label)
        }
        Cmd::Line { game, json } => {
            let engine = connect(&cli)?;
            let screen = open_game(&engine, game)?;
            let stdin = std::io::stdin();
            let mut out = std::io::stdout();
            let errors = line::run(
                &engine,
                &screen,
                stdin.lock(),
                &mut out,
                line::Options { json: *json },
                &Reconnect::default(),
            )?;
            if errors > 0 {
                std::process::exit(2);
            }
            Ok(())
        }
        Cmd::Games => {
            let engine = connect(&cli)?;
            for g in engine.games().map_err(engine_error)? {
                let state = if g.over { "over" } else { "playing" };
                println!("{}  {}  {}  {}", g.id, g.world.title, state, g.started_at);
            }
            Ok(())
        }
        Cmd::Worlds => {
            let engine = connect(&cli)?;
            for w in engine.worlds().map_err(engine_error)? {
                println!("{}  {}\n    {}", w.id, w.title, w.summary);
            }
            Ok(())
        }
        Cmd::New { world } => {
            let engine = connect(&cli)?;
            let screen = engine.start_game(world).map_err(engine_error)?;
            println!("Started {} in {}.", screen.game.id, screen.game.world.title);
            println!("Play it with `ta play {}`.", screen.game.id);
            Ok(())
        }
        Cmd::Whoami => {
            let engine = connect(&cli)?;
            let s = engine.service().map_err(engine_error)?;
            let spend = &s.player.spend;
            println!("player   {}", s.player.name);
            println!(
                "engine   {} {} (protocol {} v{})",
                s.engine.name, s.engine.version, s.protocol, s.version
            );
            println!(
                "spend    ${:.2} used, ${:.2} held, of ${:.2} until {}",
                spend.used_usd, spend.held_usd, spend.limit_usd, spend.period_ends_at
            );
            Ok(())
        }
        Cmd::Login => login(&cli),
        Cmd::Logout => {
            config::forget_token()?;
            println!("The stored token is gone.");
            Ok(())
        }
        Cmd::Config => {
            let file = file_config()?;
            let path = config::config_path();
            println!(
                "config file  {}",
                path.as_ref()
                    .map_or("(none on this system)".into(), |p| p.display().to_string())
            );
            let url_source = if cli.engine.is_some() {
                "--engine"
            } else if env(ENV_URL).is_some() {
                ENV_URL
            } else if file.engine_url.is_some() {
                "config file"
            } else {
                "not set"
            };
            let url = config::resolve_url(cli.engine.as_deref(), env(ENV_URL).as_deref(), &file);
            println!("engine       {} ({url_source})", url.unwrap_or_default());
            let token_source = if cli.token.is_some() {
                "--token"
            } else if env(ENV_TOKEN).is_some() {
                ENV_TOKEN
            } else if config::load_token().is_some() {
                "stored"
            } else {
                "not set"
            };
            println!("token        {token_source}");
            Ok(())
        }
        Cmd::Upgrade { check } => upgrade::run(*check),
    }
}

fn login(cli: &Cli) -> anyhow::Result<()> {
    let path = config::config_path().context("no config directory on this system")?;
    let mut file = config::load_file(&path)?;
    if let Some(url) = cli.engine.as_deref().or(env(ENV_URL).as_deref()) {
        HttpEngine::new(url, "").map_err(anyhow::Error::msg)?;
        file.engine_url = Some(url.trim().to_string());
        config::save_file(&path, &file)?;
        println!("Saved the engine address to {}.", path.display());
    } else if file.engine_url.is_none() {
        bail!("pass the engine's address: `ta login --engine <URL>`");
    }
    let token = match cli.token.clone().or_else(|| env(ENV_TOKEN)) {
        Some(t) => t,
        None if std::io::stdin().is_terminal() => {
            print!("Player token (from the engine's operator): ");
            std::io::stdout().flush()?;
            rpassword::read_password()?
        }
        None => {
            let mut t = String::new();
            std::io::stdin().read_line(&mut t)?;
            t
        }
    };
    let token = token.trim();
    if token.is_empty() {
        bail!("no token given");
    }
    let url = file.engine_url.clone().unwrap_or_default();
    let engine = HttpEngine::new(&url, token).map_err(anyhow::Error::msg)?;
    let service = engine.service().map_err(engine_error)?;
    let store = config::store_token(token)?;
    println!(
        "Signed in as {} on {} {}. The token is kept in {store}.",
        service.player.name, service.engine.name, service.engine.version
    );
    Ok(())
}
