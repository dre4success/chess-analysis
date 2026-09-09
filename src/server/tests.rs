use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn app(dir: &std::path::Path) -> Arc<App> {
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>Tempo</title>",
    )
    .unwrap();
    Arc::new(App {
        store: store::Store::open(&dir.join("tempo.sqlite3")).unwrap(),
        config: Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: dir.into(),
            ui_dir: dir.into(),
            engine: PathBuf::from("unused-in-api-tests"),
        },
        notify: Notify::new(),
        stopping: AtomicBool::new(false),
        engine_version: "Stockfish test".into(),
        core_hash: "test-core".into(),
    })
}

fn fixture() -> PlayerData {
    let value: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rapid-12.json")).unwrap();
    let mut games = value["games"].as_array().unwrap().clone();
    games.sort_by_key(|g| g["pgn"].as_str().unwrap().len());
    games.truncate(1);
    PlayerData {
        profile: json!({"username":"dre4success007"}),
        games,
        pace: "rapid".into(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
        archives_read: 1,
        warning: None,
        skipped: 0,
    }
}

#[test]
fn queue_deduplicates_recovers_and_keeps_cancellation_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.sqlite3");
    let id;
    {
        let store = store::Store::open(&path).unwrap();
        let first = store
            .enqueue(Job::new("dre".into(), "rapid".into()))
            .unwrap();
        id = first.id;
        assert_eq!(
            id,
            store
                .enqueue(Job::new("dre".into(), "rapid".into()))
                .unwrap()
                .id
        );
        store
            .update(id, |j| {
                j.status = "running".into();
                j.progress.completed = 1;
                j.data = Some(fixture());
            })
            .unwrap();
    }
    let store = store::Store::open(&path).unwrap();
    let resumed = store.next().unwrap().unwrap();
    assert_eq!(resumed.id, id);
    assert_eq!(resumed.progress.completed, 1);
    assert!(resumed.data.is_some());
    store.update(id, |j| j.status = "cancelled".into()).unwrap();
    store.update(id, |j| j.status = "complete".into()).unwrap();
    assert_eq!(store.get(id).unwrap().unwrap().status, "cancelled");
    assert!(store.next().unwrap().is_none());
    assert_ne!(
        id,
        store
            .enqueue(Job::new("dre".into(), "rapid".into()))
            .unwrap()
            .id
    );
}

#[test]
fn queue_has_a_bounded_backlog() {
    let dir = tempfile::tempdir().unwrap();
    let store = store::Store::open(&dir.path().join("db")).unwrap();
    for i in 0..32 {
        store
            .enqueue(Job::new(format!("player{i}"), "rapid".into()))
            .unwrap();
    }
    assert!(
        store
            .enqueue(Job::new("extra".into(), "rapid".into()))
            .is_err()
    );
    assert!(
        store
            .enqueue(Job::new("player1".into(), "rapid".into()))
            .is_ok()
    );
}

async fn request(router: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}

#[tokio::test]
async fn api_creates_reopens_extends_and_cancels_saved_reviews() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let router = router(app.clone());
    let (status, job) = request(
        &router,
        "POST",
        "/api/reviews",
        json!({"username":" Dre4Success007 "}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(job["username"], "dre4success007");
    let id = job["id"].as_i64().unwrap();
    assert_eq!(
        request(&router, "GET", &format!("/api/reviews/{id}"), json!(null))
            .await
            .1["id"],
        id
    );
    assert_eq!(
        request(
            &router,
            "POST",
            "/api/reviews",
            json!({"username":"dre4success007"})
        )
        .await
        .1["id"],
        id
    );
    app.store
        .update(id, |j| {
            j.data = Some(fixture());
            j.status = "complete".into();
        })
        .unwrap();
    let url = fixture().games[0]["url"].clone();
    let (status, child) = request(
        &router,
        "POST",
        "/api/reviews",
        json!({"username":"dre4success007","parent_id":id,"game_urls":[url]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(child["id"], id);
    assert!(child["data"]["games"].is_array());
    let child = child["id"].as_i64().unwrap();
    assert_eq!(
        request(
            &router,
            "POST",
            &format!("/api/reviews/{child}/cancel"),
            json!({})
        )
        .await
        .1["status"],
        "cancelled"
    );
    let recent = request(&router, "GET", "/api/reviews", json!(null)).await.1;
    assert_eq!(recent["reviews"].as_array().unwrap().len(), 2);
    assert!(recent["reviews"][0].get("data").is_none());
}

#[tokio::test]
async fn api_rejects_untrusted_selections_and_does_not_fallback_to_html() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let router = router(app.clone());
    for body in [
        json!({"username":"../root"}),
        json!({"username":"dre","pace":"daily"}),
        json!({"username":"dre","game_urls":["http://localhost/secret"]}),
    ] {
        assert_eq!(
            request(&router, "POST", "/api/reviews", body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut parent = Job::new("dre4success007".into(), "rapid".into());
    parent.status = "complete".into();
    parent.data = Some(fixture());
    let id = app.store.enqueue(parent).unwrap().id;
    assert_eq!(request(&router,"POST","/api/reviews",json!({"username":"dre4success007","parent_id":id,"game_urls":["https://www.chess.com/game/live/1"]})).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(
        request(
            &router,
            "POST",
            "/api/reviews",
            json!({"username":"someone-else","parent_id":id})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reviews")
                .header("content-type", "application/json")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from(r#"{"username":"dre"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        request(&router, "GET", "/api/missing", json!(null)).await.0,
        StatusCode::NOT_FOUND
    );
    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.headers()[header::CACHE_CONTROL], "no-store");
    let home = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    assert!(
        home.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn static_cache_policy_keeps_html_current_and_does_not_cache_missing_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let router = router(app(dir.path()));
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(
        dir.path().join("assets/rating-panel-new12345.js"),
        "export default 1;",
    )
    .unwrap();
    for (path, status, policy) in [
        ("/", StatusCode::OK, "no-store"),
        ("/?review=7", StatusCode::OK, "no-store"),
        ("/index.html", StatusCode::OK, "no-store"),
        ("/api/health", StatusCode::OK, "no-store"),
        ("/api/missing", StatusCode::NOT_FOUND, "no-store"),
        (
            "/assets/rating-panel-old12345.js",
            StatusCode::NOT_FOUND,
            "no-store",
        ),
        (
            "/assets/rating-panel-new12345.js",
            StatusCode::OK,
            "public, max-age=31536000, immutable",
        ),
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], policy, "{path}");
    }
}

#[test]
#[ignore = "requires STOCKFISH executable; real native pipeline, persistence and cache invalidation"]
fn native_job_reuses_verified_cache_and_invalidates_changed_core() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    Arc::get_mut(&mut app).unwrap().config.engine =
        std::env::var("STOCKFISH").expect("set STOCKFISH").into();
    let mut job = Job::new("dre4success007".into(), "rapid".into());
    job.data = Some(fixture());
    let job = app.store.enqueue(job).unwrap();
    let id = job.id;
    run_job(&app, job);
    let first = app.store.get(id).unwrap().unwrap();
    assert_eq!(first.status, "complete", "{:?}", first.error);
    assert!(first.progress.positions > 0);
    review::validate(first.review.clone().unwrap()).unwrap();
    let mut retry = Job::new("dre4success007".into(), "rapid".into());
    retry.data = first.data.clone();
    let retry = app.store.enqueue(retry).unwrap();
    let retry_id = retry.id;
    run_job(&app, retry);
    let cached = app.store.get(retry_id).unwrap().unwrap();
    assert_eq!(cached.status, "complete");
    assert_eq!(cached.progress.positions, 0);
    assert_eq!(first.review.unwrap().games, cached.review.unwrap().games);
    Arc::get_mut(&mut app).unwrap().core_hash = "updated-core".into();
    let mut retry = Job::new("dre4success007".into(), "rapid".into());
    retry.data = first.data;
    let retry = app.store.enqueue(retry).unwrap();
    let id = retry.id;
    run_job(&app, retry);
    assert!(app.store.get(id).unwrap().unwrap().progress.positions > 0);
}
