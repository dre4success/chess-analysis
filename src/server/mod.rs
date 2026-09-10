//! Same-origin web service and a durable, single-worker review queue.
mod import;
mod store;

use crate::{
    analysis::AnalysisConfig,
    engine::{Analysis, EngineError, Nodes, PositionEngine, SearchContext, UciEngine},
    fetch, patterns,
    pipeline::{self, Input},
    render,
    review::{self, Review, ReviewMode},
    stockfish,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as RoutePath, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub ui_dir: PathBuf,
    pub engine: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerData {
    pub profile: Value,
    pub games: Vec<Value>,
    pub pace: String,
    pub fetched_at: String,
    pub archives_read: usize,
    pub warning: Option<String>,
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub completed: usize,
    pub total: usize,
    pub positions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub username: String,
    pub pace: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub progress: Progress,
    pub message: String,
    pub error: Option<String>,
    pub data: Option<PlayerData>,
    pub review: Option<Review>,
    pub target_urls: Vec<String>,
    #[serde(default)]
    pub engine_key: Option<String>,
}

impl Job {
    fn new(username: String, pace: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: 0,
            username,
            pace,
            status: "queued".into(),
            created_at: now.clone(),
            updated_at: now,
            progress: Progress::default(),
            message: "Waiting for the review worker".into(),
            error: None,
            data: None,
            review: None,
            target_urls: vec![],
            engine_key: None,
        }
    }
}

struct App {
    store: store::Store,
    config: Config,
    notify: Notify,
    stopping: AtomicBool,
    engine_version: String,
    core_hash: String,
}

impl App {
    fn cancelled(&self, id: i64) -> bool {
        self.stopping.load(Ordering::Relaxed)
            || !matches!(self.store.get(id), Ok(Some(job)) if job.status != "cancelled")
    }
}

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<Box<dyn std::error::Error + Send + Sync>> for ApiError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        eprintln!("API error: {error}");
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "The review service is busy. Please try again shortly.".into(),
        )
    }
}
type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateReview {
    username: String,
    #[serde(default = "rapid")]
    pace: String,
    parent_id: Option<i64>,
    #[serde(default)]
    game_urls: Vec<String>,
}
fn rapid() -> String {
    "rapid".into()
}

async fn create(State(app): State<Arc<App>>, Json(input): Json<CreateReview>) -> ApiResult<Job> {
    let user = fetch::username(input.username.trim())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
    if !["rapid", "blitz", "bullet"].contains(&input.pace.as_str()) || input.game_urls.len() > 40 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Choose rapid, blitz or bullet, with at most 40 games.".into(),
        ));
    }
    let mut job = Job::new(user, input.pace);
    if let Some(parent_id) = input.parent_id {
        let parent = app
            .store
            .get(parent_id)?
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Saved review not found.".into()))?;
        if parent.username != job.username || parent.pace != job.pace {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "The saved review belongs to another player or time format.".into(),
            ));
        }
        let data = parent.data.ok_or_else(|| {
            ApiError(
                StatusCode::CONFLICT,
                "The games have not finished importing yet.".into(),
            )
        })?;
        if input
            .game_urls
            .iter()
            .any(|url| !data.games.iter().any(|g| g["url"].as_str() == Some(url)))
        {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Choose games from this saved review.".into(),
            ));
        }
        job.data = Some(data);
        // Keep previously completed games when extending a saved review.
        job.review = parent.review;
        job.engine_key = parent.engine_key;
    } else if !input.game_urls.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "A saved review is required to select games.".into(),
        ));
    }
    job.target_urls = input.game_urls;
    job.target_urls.sort();
    job.target_urls.dedup();
    let job = app.store.enqueue(job)?;
    app.notify.notify_one();
    Ok(Json(job))
}

async fn get_job(State(app): State<Arc<App>>, RoutePath(id): RoutePath<i64>) -> ApiResult<Job> {
    Ok(Json(app.store.get(id)?.ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            "This saved review was not found.".into(),
        )
    })?))
}
#[derive(Deserialize)]
struct HistoryQuery {
    usernames: Option<String>,
    #[serde(default)]
    offset: i64,
    #[serde(default = "history_limit")]
    limit: i64,
}
fn history_limit() -> i64 {
    8
}
async fn recent(
    State(app): State<Arc<App>>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Value> {
    if query.offset < 0 || !(1..=100).contains(&query.limit) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Use a non-negative offset and a limit between 1 and 100.".into(),
        ));
    }
    let mut usernames = match query.usernames.as_deref().map(str::trim) {
        None | Some("") => Vec::new(),
        Some(value) => value
            .split(',')
            .map(|username| {
                fetch::username(username.trim())
                    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    usernames.sort();
    usernames.dedup();
    if usernames.len() > 100 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Choose at most 100 player usernames.".into(),
        ));
    }
    // The browser supplies its own search history. An unscoped request must not
    // expose the shared installation's entire saved library.
    let (page, total) = app.store.history(&usernames, query.offset, query.limit)?;
    let jobs: Vec<_> = page.iter().map(|j| {
        let dates: Vec<_> = j.data.iter().flat_map(|d| &d.games)
            .filter_map(|g| g["end_time"].as_i64()).collect();
        json!({
            "id":j.id,"username":j.username,"pace":j.pace,"status":j.status,
            "created_at":j.created_at,"progress":j.progress,
            "imported_games":j.data.as_ref().map(|d| d.games.len()),
            "reviewed_games":j.review.as_ref().map(|r| r.games.len()),
            "first_game_at":dates.iter().min(),"last_game_at":dates.iter().max(),
            "findings":j.review.as_ref().map(|r| r.games.iter().map(|g|g.findings.len()).sum::<usize>())
        })
    }).collect();
    Ok(Json(json!({
        "reviews":jobs,"offset":query.offset,"limit":query.limit,"total":total,
        "has_more": query.offset < total.saturating_sub(page.len() as i64)
    })))
}

async fn digest(
    State(app): State<Arc<App>>,
    RoutePath(id): RoutePath<i64>,
) -> std::result::Result<Response, ApiError> {
    let Json(job) = get_job(State(app), RoutePath(id)).await?;
    let saved = job.review.filter(|r| !r.games.is_empty()).ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "The digest will be available after the first game finishes reviewing.".into(),
        )
    })?;
    let validated = review::validate(saved).map_err(|e| {
        eprintln!("Review {id} cannot be exported: {e}");
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "This saved review could not be validated for export.".into(),
        )
    })?;
    let mut body = render::digest(&validated);
    if job.status != "complete" {
        body.push_str("\nThis is a partial review containing the games completed so far.\n");
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"tempo-review-{id}.md\""),
            ),
        ],
        body,
    )
        .into_response())
}
async fn cancel(State(app): State<Arc<App>>, RoutePath(id): RoutePath<i64>) -> ApiResult<Job> {
    let _ = get_job(State(app.clone()), RoutePath(id)).await?;
    app.store.update(id, |j| {
        if ["queued", "running"].contains(&j.status.as_str()) {
            j.status = "cancelled".into();
            j.message = "Review stopped. Completed games are saved.".into();
        }
    })?;
    get_job(State(app), RoutePath(id)).await
}
async fn health(State(app): State<Arc<App>>) -> ApiResult<Value> {
    let queue = app.store.queue_status()?;
    let stopping = app.stopping.load(Ordering::Relaxed);
    if stopping {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "The review service is shutting down.".into(),
        ));
    }
    Ok(Json(
        json!({"status":"ok","engine":app.engine_version,"runtime":"native","worker":"single","persistence":"sqlite","queue":queue}),
    ))
}

fn cross_site(headers: &HeaderMap) -> bool {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return true;
    }
    let Some(origin) = headers.get(header::ORIGIN) else {
        // Non-browser clients, including the CLI, do not send Origin.
        return false;
    };
    let same_origin = || {
        let origin = url::Url::parse(origin.to_str().ok()?).ok()?;
        if !matches!(origin.scheme(), "http" | "https") {
            return None;
        }
        let host = headers.get(header::HOST)?.to_str().ok()?;
        let target = url::Url::parse(&format!("{}://{host}", origin.scheme())).ok()?;
        Some(
            origin.host_str() == target.host_str()
                && origin.port_or_known_default() == target.port_or_known_default(),
        )
    };
    same_origin() != Some(true)
}

async fn protect_mutations(request: Request, next: Next) -> Response {
    if !matches!(request.method().as_str(), "GET" | "HEAD" | "OPTIONS")
        && cross_site(request.headers())
    {
        return ApiError(
            StatusCode::FORBIDDEN,
            "Open Tempo directly to change a review.".into(),
        )
        .into_response();
    }
    next.run(request).await
}

async fn cache_control(request: Request, next: Next) -> Response {
    // Vite puts content-hashed files in /assets/. HTML must always come from
    // the current build, and missing chunks must never become cached 404s.
    let asset = request.uri().path().starts_with("/assets/");
    let mut response = next.run(request).await;
    let policy = if asset && response.status().is_success() {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(policy),
    );
    response
}

fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/reviews", get(recent).post(create))
        .route("/reviews/{id}", get(get_job))
        .route("/reviews/{id}/digest", get(digest))
        .route("/reviews/{id}/cancel", post(cancel))
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"API route not found"})),
            )
        })
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(protect_mutations));
    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(&app.config.ui_dir))
        .layer(middleware::from_fn(cache_control))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            header::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            header::HeaderValue::from_static("same-origin"),
        ))
        .with_state(app)
}

pub async fn serve(config: Config) -> Result<()> {
    if !config.ui_dir.join("index.html").is_file() {
        return Err("UI build missing. Run npm ci && npm run build in web/ first.".into());
    }
    std::fs::create_dir_all(&config.data_dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.data_dir.join("worker.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "Another Tempo server is already using this data directory")?;
    let budgets = AnalysisConfig::default();
    let (engine, metadata) = stockfish::open(
        &config.engine,
        budgets.scan,
        budgets.deep,
        Duration::from_secs(30),
    )?;
    drop(engine);
    let db = store::Store::open(&config.data_dir.join("tempo.sqlite3"))?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    eprintln!(
        "Tempo at http://{} · {} · reviews saved in {}",
        listener.local_addr()?,
        metadata.version,
        config.data_dir.display()
    );
    let app = Arc::new(App {
        store: db,
        config,
        notify: Notify::new(),
        stopping: AtomicBool::new(false),
        engine_version: metadata.version,
        core_hash: stockfish::file_sha256(&std::env::current_exe()?)?,
    });
    let worker_app = app.clone();
    let worker = tokio::spawn(async move {
        while !worker_app.stopping.load(Ordering::Relaxed) {
            match worker_app.store.next() {
                Ok(Some(job)) => {
                    let work = worker_app.clone();
                    let id = job.id;
                    let result = tokio::task::spawn_blocking(move || run_job(&work, job)).await;
                    if let Err(error) = result {
                        let _ = worker_app.store.update(id, |j| {
                            j.status = "failed".into();
                            j.error = Some(format!("Review worker stopped: {error}"));
                        });
                    }
                }
                Ok(None) => {
                    tokio::select! { _ = worker_app.notify.notified() => {}, _ = tokio::time::sleep(Duration::from_secs(2)) => {} }
                }
                Err(error) => {
                    eprintln!("Queue error: {error}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });
    let signal_app = app.clone();
    let result = axum::serve(listener, router(app.clone()))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            signal_app.stopping.store(true, Ordering::Relaxed);
            signal_app.notify.notify_one();
        })
        .await;
    app.stopping.store(true, Ordering::Relaxed);
    app.notify.notify_one();
    worker.await?;
    drop(lock);
    result?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn run_job(app: &App, mut job: Job) {
    let result = analyse_job(app, &mut job);
    if let Err(error) = result
        && !app.cancelled(job.id)
    {
        eprintln!("Review {} failed: {error}", job.id);
        let _ = app.store.update(job.id, |j| {
            j.status = "failed".into();
            j.message = "Your completed games are saved. You can retry this review.".into();
            j.error = Some(error.to_string());
        });
    }
}

fn analyse_job(app: &App, job: &mut Job) -> Result<()> {
    if app.cancelled(job.id) {
        return Ok(());
    }
    app.store.update(job.id, |j| {
        j.status = "running".into();
        j.error = None;
        j.message = "Importing your completed Chess.com games".into();
    })?;
    if job.data.is_none() {
        job.data = Some(import::player(
            &job.username,
            &job.pace,
            &app.config.data_dir.join("archives"),
            || app.cancelled(job.id),
        )?);
        app.store.update(job.id, |j| j.data = job.data.clone())?;
    }
    if app.cancelled(job.id) {
        return Ok(());
    }
    let data = job.data.as_ref().ok_or("missing imported games")?;
    let targets: Vec<_> = data
        .games
        .iter()
        .filter(|g| {
            job.target_urls.is_empty()
                || g["url"]
                    .as_str()
                    .is_some_and(|url| job.target_urls.iter().any(|s| s == url))
        })
        .take(if job.target_urls.is_empty() { 5 } else { 40 })
        .cloned()
        .collect();
    let budgets = AnalysisConfig::default();
    let (engine, metadata) = stockfish::open(
        &app.config.engine,
        budgets.scan,
        budgets.deep,
        Duration::from_secs(30),
    )?;
    let engine_key = stockfish::bytes_sha256(&serde_json::to_vec(&(&app.core_hash, &metadata))?);
    // A continued review may have used an older engine. Reuse only when its
    // recorded metadata matches; per-game cache additionally pins the core.
    let previous = job
        .review
        .take()
        .filter(|r| r.engine == metadata && job.engine_key.as_ref() == Some(&engine_key))
        .map(|r| r.games)
        .unwrap_or_default();
    let mut review = Review {
        schema_version: review::SCHEMA_VERSION.into(),
        user: job.username.clone(),
        generated: chrono::Utc::now().to_rfc3339(),
        mode: ReviewMode::Verified,
        engine: metadata,
        games: previous,
        patterns: vec![],
    };
    review.patterns = patterns::aggregate(&review.games);
    app.store.update(job.id, |j| {
        j.progress = Progress {
            total: targets.len(),
            ..Default::default()
        };
        j.review = Some(review.clone());
        j.engine_key = Some(engine_key.clone());
    })?;
    let mut engine = ReportingEngine {
        inner: engine,
        app,
        id: job.id,
        positions: 0,
    };
    for (index, raw) in targets.iter().enumerate() {
        if app.cancelled(job.id) {
            return Ok(());
        }
        let game: fetch::ApiGame = serde_json::from_value(raw.clone())?;
        let input = Input {
            game: fetch::verify(&game, chrono::Utc::now().timestamp())?,
            url: game.url.clone(),
            hash: stockfish::bytes_sha256(game.pgn.as_bytes()),
        };
        let key = stockfish::bytes_sha256(&serde_json::to_vec(&(
            &engine_key,
            &job.username,
            &input.url,
            &input.hash,
        ))?);
        app.store.update(job.id, |j| {
            j.message = format!("Reviewing game {} of {}", index + 1, targets.len())
        })?;
        let result = match app.store.cached(&key)? {
            Some(result) => result,
            None => pipeline::game_review(&input, &job.username, &mut engine, budgets)
                .map_err(|e| e.to_string())?,
        };
        if app.cancelled(job.id) {
            return Ok(());
        }
        review.games.retain(|g| g.url != result.url);
        review.games.push(result.clone());
        review.patterns = patterns::aggregate(&review.games);
        review::validate(review.clone())?;
        app.store.cache(&key, &result)?;
        app.store.update(job.id, |j| {
            j.review = Some(review.clone());
            j.progress.completed = index + 1;
            j.progress.positions = engine.positions;
        })?;
    }
    app.store.update(job.id, |j| {
        j.status = "complete".into();
        j.message = if targets.is_empty() {
            "No completed rated games found in the latest eight active months.".into()
        } else {
            "Your review is ready. Every completed game is saved.".into()
        };
    })?;
    Ok(())
}

struct ReportingEngine<'a> {
    inner: UciEngine,
    app: &'a App,
    id: i64,
    positions: usize,
}
impl PositionEngine for ReportingEngine<'_> {
    fn analyse(
        &mut self,
        position: &shakmaty::Chess,
        user: shakmaty::Color,
        nodes: Nodes,
    ) -> std::result::Result<Analysis, EngineError> {
        self.analyse_with_history(
            SearchContext {
                position,
                initial: position,
                moves: &[],
            },
            user,
            nodes,
        )
    }

    fn analyse_with_history(
        &mut self,
        context: SearchContext<'_>,
        user: shakmaty::Color,
        nodes: Nodes,
    ) -> std::result::Result<Analysis, EngineError> {
        if self.app.cancelled(self.id) {
            return Err(EngineError::Closed);
        }
        let result = self.inner.analyse_with_history(context, user, nodes)?;
        self.positions += 1;
        if self.positions.is_multiple_of(8) {
            self.app
                .store
                .update(self.id, |j| j.progress.positions = self.positions)
                .map_err(|e| EngineError::Protocol(e.to_string()))?;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
