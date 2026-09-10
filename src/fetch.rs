use crate::{
    completion::{CompletedGame, CompletionPolicy, LocalPgnPolicy},
    pgn,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("player or archive not found (404): {0}")]
    NotFound(String),
    #[error("Chess.com rate limited this request (429); retry later")]
    RateLimited,
    #[error("Chess.com request timed out")]
    Timeout,
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("malformed archive: {0}")]
    Malformed(String),
    #[error("cache I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("no cached data for {0}; offline mode cannot fetch it")]
    Offline(String),
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiPlayer {
    pub username: String,
    pub result: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiGame {
    pub pgn: String,
    pub url: String,
    pub end_time: i64,
    pub rules: String,
    pub time_class: String,
    pub rated: bool,
    pub white: ApiPlayer,
    pub black: ApiPlayer,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Month {
    pub games: Vec<ApiGame>,
}
#[derive(Debug, Deserialize, Serialize)]
#[cfg(not(target_arch = "wasm32"))]
struct Archives {
    archives: Vec<String>,
}

pub fn username(value: &str) -> Result<String, FetchError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(FetchError::Malformed("invalid username".into()));
    }
    Ok(value.to_ascii_lowercase())
}
pub fn verify(game: &ApiGame, now: i64) -> Result<CompletedGame, String> {
    if game.end_time <= 0 || game.end_time > now || game.rules != "chess" {
        return Err("missing/pending completion time or unsupported rules".into());
    }
    let url = url::Url::parse(&game.url).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.chess.com" | "chess.com"))
        || url.path().contains("/game/ongoing/")
        || url.path().starts_with("/play/online")
        || url.query_pairs().any(|(_, v)| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "live" | "ongoing" | "playing"
            )
        })
    {
        return Err("unsupported or ongoing game URL".into());
    }
    let mut raw = pgn::parse_many(game.pgn.as_bytes()).map_err(|e| e.to_string())?;
    if raw.len() != 1 {
        return Err("API entry must contain exactly one game".into());
    }
    let completed = LocalPgnPolicy
        .verify(raw.remove(0))
        .map_err(|e| e.to_string())?;
    let h = completed.headers();
    let termination = h
        .get("Termination")
        .ok_or("missing Termination")?
        .to_ascii_lowercase();
    if termination.is_empty()
        || ["unterminated", "ongoing", "in progress", "unknown"]
            .iter()
            .any(|s| termination.contains(s))
    {
        return Err("unconfirmed Termination".into());
    }
    for (tag, player) in [("White", &game.white), ("Black", &game.black)] {
        if !h
            .get(tag)
            .is_some_and(|v| v.eq_ignore_ascii_case(&player.username))
        {
            return Err("API and PGN players disagree".into());
        }
    }
    let draw = |r: &str| {
        matches!(
            r,
            "agreed"
                | "repetition"
                | "stalemate"
                | "insufficient"
                | "50move"
                | "timevsinsufficient"
        )
    };
    let lost = |r: &str| {
        matches!(
            r,
            "checkmated" | "timeout" | "resigned" | "lose" | "abandoned"
        )
    };
    let expected = if game.white.result == "win" && lost(&game.black.result) {
        "1-0"
    } else if game.black.result == "win" && lost(&game.white.result) {
        "0-1"
    } else if draw(&game.white.result) && draw(&game.black.result) {
        "1/2-1/2"
    } else {
        return Err("API does not confirm a finished result".into());
    };
    if h.get("Result").map(String::as_str) != Some(expected) {
        return Err("API and PGN results disagree".into());
    }
    Ok(completed)
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::review::atomic_write;
    use std::{
        path::{Path, PathBuf},
        time::{Duration, SystemTime},
    };
    /// A month is final only when this copy was fetched after its closing day.
    /// The extra day allows Chess.com's archive publication to settle. A copy
    /// fetched during an active month must be refreshed, even months later.
    pub fn archive_cache_is_finalized(month: &str, cached_at: SystemTime, now: SystemTime) -> bool {
        let month = month.replace('-', "/");
        let Some(final_after) =
            chrono::NaiveDate::parse_from_str(&format!("{month}/01"), "%Y/%m/%d")
                .ok()
                .and_then(|date| date.checked_add_months(chrono::Months::new(1)))
                .and_then(|date| date.succ_opt())
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        else {
            return false;
        };
        let cached_at: chrono::DateTime<chrono::Utc> = cached_at.into();
        let now: chrono::DateTime<chrono::Utc> = now.into();
        cached_at >= final_after.and_utc() && cached_at <= now
    }
    pub trait Transport {
        fn get(&mut self, url: &str) -> Result<Vec<u8>, FetchError>;
    }
    pub struct Http {
        agent: ureq::Agent,
    }
    impl Http {
        pub fn new(user: &str) -> Self {
            let config = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(30)))
                .user_agent(format!(
                    "chess-review/0.1 (personal post-game review; username: {user})"
                ))
                .build();
            Self {
                agent: config.into(),
            }
        }
    }
    impl Transport for Http {
        fn get(&mut self, url: &str) -> Result<Vec<u8>, FetchError> {
            let mut response = self.agent.get(url).call().map_err(|e| match e {
                ureq::Error::StatusCode(404) => FetchError::NotFound(url.into()),
                ureq::Error::StatusCode(429) => FetchError::RateLimited,
                ureq::Error::Timeout(_) => FetchError::Timeout,
                other => FetchError::Http(other.to_string()),
            })?;
            response
                .body_mut()
                .with_config()
                .limit(64 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| FetchError::Http(e.to_string()))
        }
    }
    pub struct Client<T> {
        pub transport: T,
        pub cache: PathBuf,
        pub offline: bool,
    }
    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, FetchError> {
        serde_json::from_slice(bytes).map_err(|e| FetchError::Malformed(e.to_string()))
    }
    impl<T: Transport> Client<T> {
        fn load(&mut self, url: &str, path: &Path, immutable: bool) -> Result<Vec<u8>, FetchError> {
            if (immutable || self.offline) && path.exists() {
                return Ok(std::fs::read(path)?);
            }
            if self.offline {
                return Err(FetchError::Offline(url.into()));
            }
            let bytes = self.transport.get(url)?;
            // Decode at least JSON before replacing cache. Typed validation is done by callers.
            let _: serde_json::Value = decode(&bytes)?;
            Ok(bytes)
        }
        pub fn recent(
            &mut self,
            user: &str,
            last: usize,
            current_month: &str,
        ) -> Result<Vec<ApiGame>, FetchError> {
            self.recent_at(user, last, current_month, SystemTime::now())
        }

        pub(super) fn recent_at(
            &mut self,
            user: &str,
            last: usize,
            current_month: &str,
            now: SystemTime,
        ) -> Result<Vec<ApiGame>, FetchError> {
            let user = username(user)?;
            let dir = self.cache.join(&user);
            std::fs::create_dir_all(&dir)?;
            let index = dir.join("archives.json");
            let root = format!("https://api.chess.com/pub/player/{user}/games/");
            let bytes = self.load(&format!("{root}archives"), &index, false)?;
            let mut archives: Archives = decode(&bytes)?;
            for url in &archives.archives {
                month_key(&root, url)?;
            }
            if !self.offline {
                atomic_write(&index, &bytes)?;
            }
            archives.archives.sort();
            archives.archives.dedup();
            let mut games = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for url in archives.archives.into_iter().rev() {
                let key = month_key(&root, &url)?;
                if key.as_str() > current_month {
                    return Err(FetchError::Malformed("future archive".into()));
                }
                let path = dir.join(format!("{key}.json"));
                let immutable =
                    key.as_str() < current_month
                        && path.metadata().and_then(|m| m.modified()).ok().is_some_and(
                            |cached_at| archive_cache_is_finalized(&key, cached_at, now),
                        );
                let bytes = self.load(&url, &path, immutable)?;
                let month: Month = decode(&bytes)?;
                if !self.offline && !immutable {
                    atomic_write(&path, &bytes)?;
                }
                for game in month.games {
                    if game.rated && game.time_class == "rapid" && seen.insert(game.url.clone()) {
                        games.push(game);
                    }
                }
                if games.len() >= last {
                    break;
                }
            }
            games.sort_by(|a, b| (b.end_time, &b.url).cmp(&(a.end_time, &a.url)));
            games.truncate(last);
            games.reverse();
            Ok(games)
        }
    }
    pub(super) fn month_key(root: &str, url: &str) -> Result<String, FetchError> {
        let tail = url
            .strip_prefix(root)
            .ok_or_else(|| FetchError::Malformed("untrusted archive URL".into()))?;
        let valid = tail.len() == 7
            && tail.as_bytes()[4] == b'/'
            && tail
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || b.is_ascii_digit())
            && ("01"..="12").contains(&&tail[5..]);
        if !valid {
            return Err(FetchError::Malformed("invalid archive month".into()));
        }
        Ok(tail.replace('/', "-"))
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(Vec<Vec<u8>>);
    impl Transport for Fake {
        fn get(&mut self, _: &str) -> Result<Vec<u8>, FetchError> {
            if self.0.is_empty() {
                panic!("unexpected network call")
            }
            Ok(self.0.remove(0))
        }
    }
    fn game() -> ApiGame {
        ApiGame {pgn: "[White \"a\"]\n[Black \"b\"]\n[Result \"1-0\"]\n[Termination \"a won by resignation\"]\n\n1. e4 e5 1-0".into(), url: "https://www.chess.com/game/live/123".into(), end_time: 100, rules:"chess".into(), rated: true, time_class:"rapid".into(), white: ApiPlayer {username:"a".into(), result:"win".into()}, black:ApiPlayer {username:"b".into(), result:"resigned".into()} }
    }
    #[test]
    fn completion_guard_accepts_finished_live_url_and_rejects_uncertainty() {
        let mut g = game();
        assert!(verify(&g, 200).is_ok());
        g.url += "?status=playing";
        assert!(verify(&g, 200).is_err());
        let mut g = game();
        g.end_time = 300;
        assert!(verify(&g, 200).is_err());
        let mut g = game();
        g.pgn = g.pgn.replace("a won by resignation", "unterminated");
        assert!(verify(&g, 200).is_err());
        let mut g = game();
        g.black.result = "win".into();
        assert!(verify(&g, 200).is_err());
    }
    #[test]
    fn closed_month_is_reusable_offline() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = serde_json::to_vec(&Month {
            games: vec![game()],
        })
        .unwrap();
        let mut client = Client {
            transport: Fake(vec![
                br#"{"archives":["https://api.chess.com/pub/player/a/games/2026/08"]}"#.to_vec(),
                fixture,
            ]),
            cache: dir.path().into(),
            offline: false,
        };
        assert_eq!(client.recent("a", 1, "2026-09").unwrap().len(), 1);
        client.offline = true;
        assert_eq!(client.recent("a", 1, "2026-09").unwrap().len(), 1);
        std::fs::write(dir.path().join("a/2026-08.json"), b"broken").unwrap();
        assert!(matches!(
            client.recent("a", 1, "2026-09"),
            Err(FetchError::Malformed(_))
        ));
    }
    fn at(value: &str) -> std::time::SystemTime {
        chrono::DateTime::parse_from_rfc3339(value).unwrap().into()
    }
    #[test]
    fn archive_finalization_requires_a_copy_fetched_after_month_end() {
        let now = at("2026-09-10T12:00:00Z");
        for cached_at in ["2026-08-15T12:00:00Z", "2026-09-01T23:59:59Z"] {
            assert!(!archive_cache_is_finalized("2026/08", at(cached_at), now));
        }
        assert!(archive_cache_is_finalized(
            "2026-08",
            at("2026-09-02T00:00:00Z"),
            now
        ));
        assert!(!archive_cache_is_finalized(
            "2026/08",
            at("2026-09-11T00:00:00Z"),
            now
        ));
        assert!(archive_cache_is_finalized(
            "2025/12",
            at("2026-01-02T00:00:00Z"),
            now
        ));
        assert!(!archive_cache_is_finalized("2026/13", now, now));
    }
    #[test]
    fn online_cli_refreshes_an_active_month_snapshot_once_after_rollover() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        let path = dir.path().join("a/2026-08.json");
        let first = game();
        let mut late = first.clone();
        late.url = "https://www.chess.com/game/live/456".into();
        late.end_time += 1;
        std::fs::write(
            &path,
            serde_json::to_vec(&Month {
                games: vec![first.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        let set_time = |time| {
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(time))
                .unwrap();
        };
        set_time(at("2026-08-15T12:00:00Z"));
        let listing =
            br#"{"archives":["https://api.chess.com/pub/player/a/games/2026/08"]}"#.to_vec();
        let mut client = Client {
            transport: Fake(vec![
                listing.clone(),
                serde_json::to_vec(&Month {
                    games: vec![first, late],
                })
                .unwrap(),
            ]),
            cache: dir.path().into(),
            offline: false,
        };
        let now = at("2026-09-10T12:00:00Z");
        assert_eq!(client.recent_at("a", 2, "2026-09", now).unwrap().len(), 2);
        assert!(client.transport.0.is_empty());
        // Stamp the fetched copy with the injected clock before the next run.
        set_time(now);
        client.transport = Fake(vec![listing]);
        assert_eq!(client.recent_at("a", 2, "2026-09", now).unwrap().len(), 2);
        assert!(client.transport.0.is_empty());
    }
    #[test]
    fn untrusted_archive_urls_are_rejected() {
        assert!(
            month_key(
                "https://api.chess.com/pub/player/a/games/",
                "https://evil.test/2026/08"
            )
            .is_err()
        );
    }
    #[test]
    fn http_errors_are_distinct_and_user_agent_is_descriptive() {
        use std::io::{Read, Write};
        for status in [404, 429, 503] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let n = stream.read(&mut request).unwrap();
                assert!(String::from_utf8_lossy(&request[..n]).contains("chess-review/0.1"));
                write!(
                    stream,
                    "HTTP/1.1 {status} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            });
            let error = Http::new("a")
                .get(&format!("http://{address}/"))
                .unwrap_err();
            match status {
                404 => assert!(matches!(error, FetchError::NotFound(_))),
                429 => assert!(matches!(error, FetchError::RateLimited)),
                _ => assert!(matches!(error, FetchError::Http(_))),
            }
            worker.join().unwrap();
        }
    }
}
