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

fn load(http: &mut impl Transport, url: &str, cache: &Path, immutable: bool) -> Result<Value> {
    let path = cache.join(format!("{}.json", bytes_sha256(url.as_bytes())));
    let fresh = path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(120));
    if (immutable || fresh) && path.is_file() {
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
    import(&mut Http::new(user), user, pace, cache, cancelled)
}

fn import(
    http: &mut impl Transport,
    user: &str,
    pace: &str,
    cache: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<PlayerData> {
    let base = format!("https://api.chess.com/pub/player/{user}");
    let listing = load(http, &format!("{base}/games/archives"), cache, false)?;
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
    let current = chrono::Utc::now().format("%Y/%m").to_string();
    let mut games = Vec::new();
    let mut seen = HashSet::new();
    let mut read = 0;
    let mut skipped = 0;
    for url in months.iter().rev().take(8) {
        if cancelled() {
            return Err("review cancelled".into());
        }
        let month = &url[prefix.len()..];
        let archive = load(http, url, cache, month < current.as_str())?;
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
            let Ok(completed) = fetch::verify(&game, chrono::Utc::now().timestamp()) else {
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
        load(http, &base, cache, false)
            .ok()
            .map(|v| json!({"username":user,"name":v["name"].as_str()}))
            .unwrap_or_else(|| json!({"username":user}))
    };
    Ok(PlayerData {
        profile,
        games,
        pace: pace.into(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
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
        let data = import(&mut http, "dre4success007", "rapid", dir.path(), || false).unwrap();
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
            import(&mut http, "dre4success007", "rapid", dir.path(), || false)
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
            assert!(import(&mut http, "dre", "rapid", dir.path(), || false).is_err());
            assert_eq!(http.requests.len(), 1);
        }
        let dir = tempfile::tempdir().unwrap();
        let mut http = HttpFixture {
            requests: vec![],
            listing: json!({"archives":["https://api.chess.com/pub/player/dre/games/2026/08"]}),
            archive: json!({}),
        };
        assert!(import(&mut http, "dre", "rapid", dir.path(), || true).is_err());
        assert_eq!(http.requests.len(), 1);
    }
}
