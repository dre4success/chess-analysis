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
            j.review = Some(crate::review::tests::sample_review());
            j.progress.completed = 1;
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
    assert_eq!(child["progress"]["completed"], 0);
    assert_eq!(child["review"]["games"].as_array().unwrap().len(), 1);
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
    let recent = request(
        &router,
        "GET",
        "/api/reviews?usernames=dre4success007",
        json!(null),
    )
    .await
    .1;
    assert_eq!(recent["reviews"].as_array().unwrap().len(), 2);
    assert!(recent["reviews"][0].get("data").is_none());
    // A continuation inherits checked games before its own batch makes progress.
    assert_eq!(recent["reviews"][0]["reviewed_games"], 1);
    assert_eq!(recent["reviews"][0]["progress"]["completed"], 0);
    assert_eq!(recent["reviews"][1]["reviewed_games"], 1);
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
async fn all_mutations_reject_cross_site_forms_and_foreign_origins() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let router = router(app.clone());
    let job = app
        .store
        .enqueue(Job::new("dre".into(), "rapid".into()))
        .unwrap();
    for path in [
        "/api/reviews".to_owned(),
        format!("/api/reviews/{}/cancel", job.id),
    ] {
        for (origin, fetch_site) in [
            ("https://foreign.example", "cross-site"),
            ("https://foreign.example", "same-site"),
            ("https://foreign.example", ""),
            ("null", ""),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&path)
                        .header("host", "tempo.example")
                        .header("origin", origin)
                        .header("sec-fetch-site", fetch_site)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("form=data"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} {origin}");
            assert_eq!(app.store.get(job.id).unwrap().unwrap().status, "queued");
        }
    }
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/reviews/{}/cancel", job.id))
                .header("host", "tempo.example")
                .header("origin", "https://tempo.example")
                .header("sec-fetch-site", "same-origin")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(app.store.get(job.id).unwrap().unwrap().status, "cancelled");
}

#[tokio::test]
async fn history_paginates_only_selected_players_and_keeps_the_default_page() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    for index in 0..23 {
        let mut job = Job::new(format!("player{}", index % 2), "rapid".into());
        job.status = "complete".into();
        job.data = Some(fixture());
        app.store.enqueue(job).unwrap();
    }
    let mut unrelated = Job::new("another-player".into(), "rapid".into());
    unrelated.status = "complete".into();
    app.store.enqueue(unrelated).unwrap();
    let router = router(app);
    let first = request(
        &router,
        "GET",
        "/api/reviews?usernames=PLAYER0,%20player1%20,Player0",
        json!(null),
    )
    .await
    .1;
    assert_eq!(first["reviews"].as_array().unwrap().len(), 8);
    assert_eq!(first["total"], 23);
    assert_eq!(first["has_more"], true);
    assert_eq!(first["reviews"][0]["id"], 23);
    assert_eq!(first["reviews"][0]["imported_games"], 1);
    assert_eq!(first["reviews"][0]["reviewed_games"], Value::Null);
    assert_eq!(
        first["reviews"][0]["first_game_at"],
        fixture().games[0]["end_time"]
    );
    assert!(first["reviews"][0].get("data").is_none());
    let mut ids = Vec::new();
    for (offset, count, has_more) in [(0, 10, true), (10, 10, true), (20, 3, false)] {
        let page = request(
            &router,
            "GET",
            &format!("/api/reviews?usernames=player0,player1&offset={offset}&limit=10"),
            json!(null),
        )
        .await
        .1;
        let jobs = page["reviews"].as_array().unwrap();
        assert_eq!(jobs.len(), count);
        assert_eq!(page["has_more"], has_more);
        ids.extend(jobs.iter().map(|j| j["id"].as_i64().unwrap()));
    }
    assert_eq!(ids, (1..=23).rev().collect::<Vec<_>>());
    let empty = request(
        &router,
        "GET",
        "/api/reviews?usernames=player0,player1&offset=100",
        json!(null),
    )
    .await
    .1;
    assert!(empty["reviews"].as_array().unwrap().is_empty());
    assert_eq!(empty["has_more"], false);
    for query in ["offset=-1", "limit=0", "limit=101"] {
        assert_eq!(
            request(
                &router,
                "GET",
                &format!("/api/reviews?{query}"),
                json!(null)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn history_requires_explicit_searches_and_isolates_player_results() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    for username in ["alice", "bob", "alice"] {
        let mut job = Job::new(username.into(), "rapid".into());
        job.status = "complete".into();
        app.store.enqueue(job).unwrap();
    }
    let router = router(app);
    for path in [
        "/api/reviews",
        "/api/reviews?usernames=",
        "/api/reviews?usernames=%20",
        "/api/reviews?usernames=unknown-player",
    ] {
        let (status, page) = request(&router, "GET", path, json!(null)).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(page["reviews"], json!([]), "{path}");
        assert_eq!(page["total"], 0, "{path}");
        assert_eq!(page["has_more"], false, "{path}");
    }
    for (username, expected_ids) in [("alice", vec![3, 1]), ("bob", vec![2])] {
        let (status, page) = request(
            &router,
            "GET",
            &format!("/api/reviews?usernames={username}"),
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["total"], expected_ids.len());
        let reviews = page["reviews"].as_array().unwrap();
        assert!(reviews.iter().all(|review| review["username"] == username));
        assert_eq!(
            reviews
                .iter()
                .map(|review| review["id"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            expected_ids
        );
    }
}

#[tokio::test]
async fn history_validates_usernames_and_bounds_unique_searches() {
    let dir = tempfile::tempdir().unwrap();
    let router = router(app(dir.path()));
    let too_many = (0..101)
        .map(|index| format!("player{index}"))
        .collect::<Vec<_>>()
        .join(",");
    for usernames in [
        "alice,,bob".to_owned(),
        "alice,".to_owned(),
        "bad%2Fname".to_owned(),
        "alice%27%20OR%201%3D1".to_owned(),
        "x".repeat(65),
        too_many,
    ] {
        assert_eq!(
            request(
                &router,
                "GET",
                &format!("/api/reviews?usernames={usernames}"),
                json!(null),
            )
            .await
            .0,
            StatusCode::BAD_REQUEST,
            "{usernames}"
        );
    }
    let exactly_100 = (0..100)
        .map(|index| format!("player{index}"))
        .collect::<Vec<_>>()
        .join(",");
    // Case variants of the same player do not consume additional slots.
    assert_eq!(
        request(
            &router,
            "GET",
            &format!("/api/reviews?usernames={exactly_100},PLAYER0"),
            json!(null),
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn digest_download_uses_saved_validated_results_and_marks_partial_reviews() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let job = app
        .store
        .enqueue(Job::new("dre".into(), "rapid".into()))
        .unwrap();
    let router = router(app.clone());
    let path = format!("/api/reviews/{}/digest", job.id);
    assert_eq!(
        request(&router, "GET", &path, json!(null)).await.0,
        StatusCode::CONFLICT
    );
    app.store
        .update(job.id, |j| {
            j.review = Some(crate::review::tests::sample_review())
        })
        .unwrap();
    let response = router
        .clone()
        .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/markdown; charset=utf-8"
    );
    assert!(
        response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("attachment; filename=")
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("# Chess review:"));
    assert!(body.contains("## Practice priorities"));
    assert!(body.contains("partial review"));
    app.store
        .update(job.id, |j| j.status = "complete".into())
        .unwrap();
    let response = router
        .clone()
        .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(!body.contains("partial review"));
    assert_eq!(
        request(&router, "GET", "/api/reviews/999/digest", json!(null))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn health_reports_queue_state_and_fails_when_storage_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let first = app
        .store
        .enqueue(Job::new("first".into(), "rapid".into()))
        .unwrap();
    app.store
        .enqueue(Job::new("second".into(), "rapid".into()))
        .unwrap();
    app.store
        .update(first.id, |j| j.status = "running".into())
        .unwrap();
    let router = router(app);
    let (status, health) = request(&router, "GET", "/api/health", json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["queue"]["queued"], 1);
    assert_eq!(health["queue"]["running"], 1);
    assert_eq!(health["queue"]["current_job"], first.id);
    // This is an isolated fixture database; no running worker uses it.
    rusqlite::Connection::open(dir.path().join("tempo.sqlite3"))
        .unwrap()
        .execute_batch("DROP TABLE jobs")
        .unwrap();
    assert_eq!(
        request(&router, "GET", "/api/health", json!(null)).await.0,
        StatusCode::SERVICE_UNAVAILABLE
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
