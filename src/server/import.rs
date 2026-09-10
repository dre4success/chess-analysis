use super::{PlayerData, Result};
use crate::{
    fetch::{self, Http, Transport},
    review::atomic_write,
    stockfish::bytes_sha256,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, SystemTime},
};

fn load(
    http: &mut impl Transport,
    url: &str,
    cache: &Path,
    month: Option<&str>,
    now: SystemTime,
) -> Result<Value> {
    let path = cache.join(format!("{}.json", bytes_sha256(url.as_bytes())));
    let cached_at = path.metadata().ok().and_then(|m| m.modified().ok());
    let fresh = cached_at
        .and_then(|t| now.duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(120));
    let finalized = month
        .zip(cached_at)
        .is_some_and(|(month, cached_at)| fetch::archive_cache_is_finalized(month, cached_at, now));
    if (finalized || fresh) && path.is_file() {
        return Ok(serde_json::from_slice(&std::fs::read(path)?)?);
    }
    let bytes = http.get(url)?;
    let value = serde_json::from_slice(&bytes)?;
    atomic_write(&path, &bytes)?;
    Ok(value)
}

pub fn player(
    user: &str,
    pace: &str,
    cache: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<PlayerData> {
    std::fs::create_dir_all(cache)?;
    import(
        &mut Http::new(user),
        user,
        pace,
        cache,
        SystemTime::now(),
        cancelled,
    )
}

fn import(
    http: &mut impl Transport,
    user: &str,
    pace: &str,
    cache: &Path,
    now: SystemTime,
    cancelled: impl Fn() -> bool,
) -> Result<PlayerData> {
    let base = format!("https://api.chess.com/pub/player/{user}");
    let listing = load(http, &format!("{base}/games/archives"), cache, None, now)?;
    let archives = listing["archives"]
        .as_array()
        .ok_or("Chess.com returned an invalid archive list")?;
    let prefix = format!("{base}/games/");
    let mut months = Vec::new();
    for item in archives {
        let url = item.as_str().ok_or("invalid archive URL")?;
        // Only these exact Chess.com month URLs can ever be fetched. Archive
        // content cannot direct our server to another host or path.
        let month = url.strip_prefix(&prefix).ok_or("unexpected archive URL")?;
        if month.len() != 7
            || month.as_bytes()[4] != b'/'
            || !month
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || b.is_ascii_digit())
            || !(1..=12).contains(&month[5..].parse::<u8>()?)
        {
            return Err("invalid archive month".into());
        }
        months.push(url.to_owned());
    }
    months.sort();
    months.dedup();
    let now_utc: chrono::DateTime<chrono::Utc> = now.into();
    let current = now_utc.format("%Y/%m").to_string();
    if months
        .iter()
        .any(|url| &url[prefix.len()..] > current.as_str())
    {
        return Err("Chess.com returned a future archive month".into());
    }
    let mut games = Vec::new();
    let mut seen = HashSet::new();
    let mut read = 0;
    let mut skipped = 0;
    for url in months.iter().rev().take(8) {
        if cancelled() {
            return Err("review cancelled".into());
        }
        let month = &url[prefix.len()..];
        let archive = load(http, url, cache, Some(month), now)?;
        read += 1;
        for raw in archive["games"]
            .as_array()
            .ok_or("Chess.com returned an invalid month")?
        {
            let Ok(game) = serde_json::from_value::<fetch::ApiGame>(raw.clone()) else {
                skipped += 1;
                continue;
            };
            if !game.rated || game.time_class != pace || game.rules != "chess" {
                continue;
            }
            if !game.white.username.eq_ignore_ascii_case(user)
                && !game.black.username.eq_ignore_ascii_case(user)
            {
                skipped += 1;
                continue;
            }
            let Ok(completed) = fetch::verify(&game, now_utc.timestamp()) else {
                skipped += 1;
                continue;
            };
            if completed.moves().is_empty() || completed.moves().len() > 600 {
                skipped += 1;
                continue;
            }
            if seen.insert(game.url.clone()) {
                games.push(raw.clone());
            }
        }
        if games.len() >= 40 {
            break;
        }
    }
    games.sort_by_key(|game| std::cmp::Reverse(game["end_time"].as_i64().unwrap_or_default()));
    games.truncate(40);
    let profile = if cancelled() {
        json!({"username":user})
    } else {
        load(http, &base, cache, None, now)
            .ok()
            .map(|v| json!({"username":user,"name":v["name"].as_str()}))
            .unwrap_or_else(|| json!({"username":user}))
    };
    Ok(PlayerData {
        profile,
        games,
        pace: pace.into(),
        fetched_at: now_utc.to_rfc3339(),
        archives_read: read,
        warning: (skipped > 0).then(|| {
            format!(
                "Skipped {skipped} entries that could not be verified as completed standard games."
            )
        }),
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_cache_refreshes_after_rollover_then_reuses_the_final_copy() {
        struct UpdatedArchive(usize);
        impl Transport for UpdatedArchive {
            fn get(&mut self, _: &str) -> std::result::Result<Vec<u8>, fetch::FetchError> {
                self.0 += 1;
                Ok(br#"{"games":["early","late"]}"#.to_vec())
            }
        }
        let at = |s| -> SystemTime { chrono::DateTime::parse_from_rfc3339(s).unwrap().into() };
        let dir = tempfile::tempdir().unwrap();
        let url = "https://api.chess.com/pub/player/dre/games/2026/08";
        let path = dir
            .path()
            .join(format!("{}.json", bytes_sha256(url.as_bytes())));
        std::fs::write(&path, br#"{"games":["early"]}"#).unwrap();
        let set_time = |time| {
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(time))
                .unwrap();
        };
        set_time(at("2026-08-15T12:00:00Z"));
        let now = at("2026-09-10T12:00:00Z");
        let mut http = UpdatedArchive(0);
        let value = load(&mut http, url, dir.path(), Some("2026/08"), now).unwrap();
        assert_eq!(value["games"].as_array().unwrap().len(), 2);
        assert_eq!(http.0, 1);
        set_time(now);
        let later = now + Duration::from_secs(3600);
        assert_eq!(
            load(&mut http, url, dir.path(), Some("2026/08"), later).unwrap(),
            value
        );
        assert_eq!(http.0, 1);
    }
    struct HttpFixture {
        requests: Vec<String>,
        listing: Value,
        archive: Value,
    }
    impl Transport for HttpFixture {
        fn get(&mut self, url: &str) -> std::result::Result<Vec<u8>, fetch::FetchError> {
            self.requests.push(url.into());
            let value = if url.ends_with("/archives") {
                &self.listing
            } else if url.ends_with("/2026/08") {
                &self.archive
            } else {
                return Ok(br#"{"username":"dre4success007"}"#.to_vec());
            };
            Ok(serde_json::to_vec(value).unwrap())
        }
    }
    #[test]
    fn importer_verifies_deduplicates_and_caches_public_games() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/rapid-12.json")).unwrap();
        let duplicate = archive["games"][0].clone();
        let mut pending = duplicate.clone();
        pending["end_time"] = json!(i64::MAX);
        archive["games"]
            .as_array_mut()
            .unwrap()
            .extend([duplicate, pending]);
        let mut http = HttpFixture {
            requests: vec![],
            listing: json!({"archives":["https://api.chess.com/pub/player/dre4success007/games/2026/08"]}),
            archive,
        };
        let data = import(
            &mut http,
            "dre4success007",
            "rapid",
            dir.path(),
            SystemTime::now(),
            || false,
        )
        .unwrap();
        assert_eq!(data.games.len(), 12);
        assert_eq!(data.skipped, 1);
        assert_eq!(data.archives_read, 1);
        assert!(
            data.games
                .windows(2)
                .all(|w| w[0]["end_time"].as_i64() >= w[1]["end_time"].as_i64())
        );
        let count = http.requests.len();
        assert_eq!(
            import(
                &mut http,
                "dre4success007",
                "rapid",
                dir.path(),
                SystemTime::now(),
                || false
            )
            .unwrap()
            .games
            .len(),
            12
        );
        assert_eq!(http.requests.len(), count);
    }
    #[test]
    fn importer_rejects_redirects_in_archive_lists_and_honours_cancellation() {
        for url in [
            "http://127.0.0.1/secret",
            "https://api.chess.com/pub/player/other/games/2026/08",
            "https://api.chess.com/pub/player/dre/games/2026/13",
            "https://api.chess.com/pub/player/dre/games/../../secret",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut http = HttpFixture {
                requests: vec![],
                listing: json!({"archives":[url]}),
                archive: json!({}),
            };
            assert!(
                import(
                    &mut http,
                    "dre",
                    "rapid",
                    dir.path(),
                    SystemTime::now(),
                    || false
                )
                .is_err()
            );
            assert_eq!(http.requests.len(), 1);
        }
        let dir = tempfile::tempdir().unwrap();
        let mut http = HttpFixture {
            requests: vec![],
            listing: json!({"archives":["https://api.chess.com/pub/player/dre/games/2026/08"]}),
            archive: json!({}),
        };
        assert!(
            import(
                &mut http,
                "dre",
                "rapid",
                dir.path(),
                SystemTime::now(),
                || true
            )
            .is_err()
        );
        assert_eq!(http.requests.len(), 1);
    }
}
