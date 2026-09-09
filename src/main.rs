use chess_review::pipeline::{Input, executable, game_review};
use chess_review::{
    analysis::AnalysisConfig,
    completion::{CompletionPolicy, LocalPgnPolicy},
    engine::Nodes,
    fetch, patterns, pgn, render,
    review::{self, Review, ReviewMode},
    stockfish,
};
use clap::{Parser, Subcommand};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Parser)]
#[command(
    version,
    about = "Review completed chess games with reproducible Stockfish evidence"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Review your latest rated rapid games, or completed games from a local PGN.
    Review {
        username: String,
        #[arg(long, default_value_t = 20)]
        last: usize,
        #[arg(long)]
        pgn: Option<PathBuf>,
        #[arg(long, default_value = "review-output")]
        output: PathBuf,
        #[arg(long, default_value = "stockfish")]
        engine: PathBuf,
        #[arg(long, default_value = ".chess-review-cache")]
        cache: PathBuf,
        #[arg(long)]
        offline: bool,
        #[arg(long, default_value_t = 150_000)]
        scan_nodes: u64,
        #[arg(long, default_value_t = 1_000_000)]
        deep_nodes: u64,
        #[arg(long, default_value_t = 200)]
        threshold_cp: u32,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Render digest.md and report.html from validated JSON alone.
    Render { input: PathBuf },
    /// Check schema, moves, FENs, SAN and PV legality.
    Validate { input: PathBuf },
    /// Serve Tempo with native Stockfish and a persistent review queue.
    Serve {
        #[arg(long, env = "TEMPO_BIND", default_value = "127.0.0.1:8080")]
        bind: std::net::SocketAddr,
        #[arg(long, env = "TEMPO_DATA_DIR", default_value = "tempo-data")]
        data_dir: PathBuf,
        #[arg(long, env = "TEMPO_UI_DIR", default_value = "web/dist/selfhost")]
        ui_dir: PathBuf,
        #[arg(long, env = "STOCKFISH_PATH", default_value = "stockfish")]
        engine: PathBuf,
    },
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::Serve {
            bind,
            data_dir,
            ui_dir,
            engine,
        } => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime
                .block_on(chess_review::server::serve(chess_review::server::Config {
                    bind,
                    data_dir,
                    ui_dir,
                    engine: executable(&engine)?,
                }))
                .map_err(|e| e as Box<dyn std::error::Error>)?;
        }
        Commands::Validate { input } => {
            let r = review::read(&input)?;
            println!("Valid: {} games", r.as_review().games.len());
        }
        Commands::Render { input } => {
            let r = review::read(&input)?;
            let dir = input.parent().unwrap_or(Path::new("."));
            review::atomic_write(&dir.join("digest.md"), render::digest(&r).as_bytes())?;
            review::atomic_write(&dir.join("report.html"), render::html(&r).as_bytes())?;
            println!("Rendered {}", dir.join("report.html").display());
        }
        Commands::Review {
            username,
            last,
            pgn,
            output,
            engine,
            cache,
            offline,
            scan_nodes,
            deep_nodes,
            threshold_cp,
            timeout_secs,
        } => {
            let user = fetch::username(&username)?;
            if last == 0 || last > 1000 || threshold_cp == 0 || timeout_secs == 0 {
                return Err("last must be 1..=1000; threshold and timeout must be positive".into());
            }
            let config = AnalysisConfig {
                scan: Nodes::new(scan_nodes)?,
                deep: Nodes::new(deep_nodes)?,
                threshold_cp,
            };
            if deep_nodes < scan_nodes {
                return Err("deep nodes must be at least scan nodes".into());
            }
            let now = chrono::Utc::now();
            let mut inputs = Vec::new();
            let mut skipped = Vec::new();
            if let Some(path) = pgn {
                let bytes = std::fs::read(&path)?;
                let hash = stockfish::bytes_sha256(&bytes);
                for (i, raw) in pgn::parse_many(&bytes)?.into_iter().enumerate() {
                    match LocalPgnPolicy.verify(raw) {
                        Ok(game) => {
                            if !["White", "Black"].iter().any(|tag| {
                                game.headers()
                                    .get(*tag)
                                    .is_some_and(|n| n.eq_ignore_ascii_case(&user))
                            }) {
                                skipped.push(format!("game {}: user is not a player", i + 1));
                                continue;
                            }
                            let url = game
                                .headers()
                                .get("Link")
                                .cloned()
                                .unwrap_or_else(|| format!("local:{hash}:{}", i + 1));
                            inputs.push(Input {
                                game,
                                url,
                                hash: hash.clone(),
                            });
                        }
                        Err(e) => skipped.push(format!("game {}: {e}", i + 1)),
                    }
                }
                if inputs.len() > last {
                    inputs.drain(..inputs.len() - last);
                }
            } else {
                let mut client = fetch::Client {
                    transport: fetch::Http::new(&user),
                    cache,
                    offline,
                };
                for raw in client.recent(&user, last, &now.format("%Y-%m").to_string())? {
                    match fetch::verify(&raw, now.timestamp()) {
                        Ok(game) => inputs.push(Input {
                            game,
                            url: raw.url,
                            hash: stockfish::bytes_sha256(raw.pgn.as_bytes()),
                        }),
                        Err(e) => skipped.push(format!("{}: {e}", raw.url)),
                    }
                }
            }
            for reason in &skipped {
                eprintln!("Skipped {reason}");
            }
            if inputs.is_empty() {
                return Err("no eligible completed games; engine was not opened".into());
            }
            // Every source entry has passed the completion guard before spawning.
            let (mut engine, mut metadata) = stockfish::open(
                &executable(&engine)?,
                config.scan,
                config.deep,
                Duration::from_secs(timeout_secs),
            )?;
            metadata.threshold_cp = threshold_cp;
            let mut games = Vec::new();
            for (i, input) in inputs.iter().enumerate() {
                eprintln!("Analysing {}/{}: {}", i + 1, inputs.len(), input.url);
                games.push(game_review(input, &user, &mut engine, config)?);
            }
            drop(engine);
            let patterns = patterns::aggregate(&games);
            let r = review::validate(Review {
                schema_version: review::SCHEMA_VERSION.into(),
                user,
                generated: now.to_rfc3339(),
                mode: ReviewMode::Verified,
                engine: metadata,
                games,
                patterns,
            })?;
            std::fs::create_dir_all(&output)?;
            review::write(&output.join("review.json"), &r)?;
            // Read back the canonical artifact; all other output depends only on it.
            let persisted = review::read(&output.join("review.json"))?;
            review::atomic_write(
                &output.join("digest.md"),
                render::digest(&persisted).as_bytes(),
            )?;
            review::atomic_write(&output.join("skipped.txt"), skipped.join("\n").as_bytes())?;
            println!(
                "Wrote {} and digest.md ({} games)",
                output.join("review.json").display(),
                persisted.as_review().games.len()
            );
        }
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
