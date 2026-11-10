use clap::{Parser, Subcommand};
use orbital::config::Config;
use orbital::{bot, db, seed};

#[derive(Parser)]
#[command(name = "orbital", about = "Orbital — themed verticals of people and their links")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the site.
    Serve,
    /// Apply migrations and exit.
    Migrate,
    /// Create the starter verticals and the owner accounts (idempotent).
    Seed,
    /// Check every outbound link and record which ones still answer.
    CheckLinks {
        /// How many links to walk. Oldest-checked first.
        #[arg(long, default_value_t = 500)]
        limit: i64,
    },
    /// Forget anonymous visitors (and their lenses) who have not been seen in a while.
    Prune {
        /// Days of silence before a visitor is forgotten.
        #[arg(long, default_value_t = 180)]
        days: i64,
    },
    /// Check the things that break quietly: database, migrations, health, backup, link checker.
    Doctor,
    /// Add links to an existing member from a file: one URL per line, optional `| label`.
    Import {
        /// The member's handle, without the @.
        #[arg(long)]
        handle: String,
        /// The file to read.
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Replay the committed golden set and print the verdict. `--emit` writes a new seed list from the
    /// rows this database holds (a power-user command: the output names rankings, so it cannot be a route).
    Eval {
        /// Emit this many seeds from the newest sessions that clicked, instead of replaying the file.
        #[arg(long)]
        emit: Option<i64>,
        /// Which golden set to replay. Defaults to the committed one.
        #[arg(long, default_value = "eval/golden.json")]
        file: std::path::PathBuf,
    },
    /// Record today's rank for every member in every vertical.
    Snapshot {
        /// The day to file the snapshot under. Defaults to today; a date in the past is how a series is
        /// backfilled without editing rows by hand.
        #[arg(long)]
        as_of: Option<String>,
    },
    /// Print what the impression ceiling sees in the last 24 hours, and the tail the rotation will fill on
    /// a date and the day after — from the date alone, so two hosts can compare notes.
    Impressions {
        /// The date to print the rotation for. Defaults to today.
        #[arg(long)]
        date: Option<String>,
    },
    /// Apply reviewed tag aliases to existing rows.
    Retag {
        #[arg(long)]
        dry_run: bool,
    },
    /// Fetch the page title and favicon for links that have none yet.
    FetchTitles {
        #[arg(long, default_value_t = 200)]
        limit: i64,
    },
    /// Render social cards into a directory (SVG always, PNG when rsvg-convert is installed).
    Og {
        #[arg(long, default_value = "static/og")]
        out: std::path::PathBuf,
        /// Only this handle.
        #[arg(long)]
        handle: Option<String>,
    },
    /// Bluesky bot and housekeeping.
    Bot {
        #[command(subcommand)]
        what: BotCmd,
    },
}

#[derive(Subcommand)]
enum BotCmd {
    /// Post up to three member spotlights.
    Spotlight {
        #[arg(long)]
        dry_run: bool,
    },
    /// Post the weekly top-5 roundup for one vertical (or today's rotation).
    Roundup {
        #[arg(long)]
        vertical: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Promote trust levels and prune old click events.
    Maintain,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "orbital=info,tower_http=warn".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    match Cli::parse().cmd {
        Cmd::Prune { days } => {
            let pool = db::connect(&cfg).await?;
            let (visitors, weights) = orbital::store::prune_visitors(&pool, days).await?;
            println!("pruned {visitors} visitor profiles and {weights} preference rows");
        }
        Cmd::CheckLinks { limit } => {
            let pool = db::connect(&cfg).await?;
            let (checked, failing, skipped) = orbital::health::run(&pool, limit).await?;
            println!("checked {checked}, failing {failing}, skipped {skipped}");
        }
        Cmd::Migrate => {
            let pool = db::connect(&cfg).await?;
            db::migrate(&pool).await?;
            println!(
                "migrated; schema version {:?}",
                db::applied_version(&pool).await?
            );
        }
        Cmd::Seed => {
            let pool = db::connect(&cfg).await?;
            db::migrate(&pool).await?;
            let report = seed::seed(&pool).await?;
            if report.credentials.is_empty() {
                println!("seed: accounts already existed, nothing minted");
            } else {
                for (handle, password, role) in &report.credentials {
                    println!("seed: {handle} ({role}) password={password}");
                }
            }
        }
        Cmd::Bot { what } => {
            let state = orbital::build_state(cfg).await?;
            match what {
                BotCmd::Spotlight { dry_run } => {
                    bot::spotlight(&state.pool, &state.cfg, &state.http, dry_run).await?
                }
                BotCmd::Roundup { vertical, dry_run } => {
                    bot::roundup(
                        &state.pool,
                        &state.cfg,
                        &state.http,
                        vertical.as_deref(),
                        dry_run,
                    )
                    .await?
                }
                BotCmd::Maintain => bot::maintenance(&state.pool).await?,
            }
        }
        Cmd::Eval { emit, file } => {
            let pool = db::connect(&cfg).await?;
            match emit {
                Some(limit) => {
                    let golden = orbital::eval::emit_seeds(&pool, limit).await?;
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&file, golden.to_json())?;
                    println!("wrote {} seeds to {}", golden.seeds.len(), file.display());
                }
                None => {
                    let raw = std::fs::read_to_string(&file).map_err(|e| {
                        anyhow::anyhow!(
                            "{}: {e}. Run `orbital eval --emit 200` against the live rows first.",
                            file.display()
                        )
                    })?;
                    let golden = orbital::eval::Golden::from_json(&raw)?;
                    let report = orbital::eval::run(&pool, &golden).await?;
                    println!("{}", report.sentence());
                    std::process::exit(report.exit_code());
                }
            }
        }
        Cmd::Impressions { date } => {
            let pool = db::connect(&cfg).await?;
            orbital::ops::impressions(&pool, &cfg, date).await?;
        }
        Cmd::Doctor => {
            let pool = db::connect(&cfg).await?;
            let failed = orbital::ops::doctor(&pool, &cfg).await?;
            std::process::exit(failed.min(125) as i32);
        }
        Cmd::Import {
            handle,
            file,
            dry_run,
        } => {
            let pool = db::connect(&cfg).await?;
            orbital::ops::import_links(&pool, &handle, &file, dry_run).await?;
        }
        Cmd::Snapshot { as_of } => {
            let pool = db::connect(&cfg).await?;
            let when = match as_of {
                Some(day) => chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                    .map_err(|e| anyhow::anyhow!("--as-of {day}: {e}"))?
                    .and_hms_opt(12, 0, 0)
                    .map(|naive| chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc))
                    .ok_or_else(|| anyhow::anyhow!("--as-of {day}: not a valid day"))?,
                None => chrono::Utc::now(),
            };
            orbital::ops::snapshot(&pool, when).await?;
        }
        Cmd::Retag { dry_run } => {
            let pool = db::connect(&cfg).await?;
            orbital::ops::retag(&pool, dry_run).await?;
        }
        Cmd::FetchTitles { limit } => {
            let pool = db::connect(&cfg).await?;
            let http = reqwest::Client::builder()
                .user_agent(concat!("orbital/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            orbital::ops::fetch_titles(&pool, &http, limit).await?;
        }
        Cmd::Og { out, handle } => {
            let pool = db::connect(&cfg).await?;
            orbital::ops::og(&pool, &out, handle.as_deref()).await?;
        }
        Cmd::Serve => serve(cfg).await?,
    }
    Ok(())
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    let addr = cfg.bind_addr.clone();
    let state = orbital::build_state(cfg).await?;
    db::migrate(&state.pool).await?;
    let app = orbital::build_router(state).await;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("orbital listening on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
