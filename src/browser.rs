//! A browser adapter for the existing review pipeline. The synchronous core is
//! replayed against a cache in three passes: request scans, request confirmations,
//! then classify. Missing searches return placeholders only to collect requests;
//! no findings are released until every requested search has a real exact PV.
use crate::{
    analysis::{self, AnalysisConfig},
    completion::CompletedGame,
    engine::{Analysis, EngineError, Nodes, PositionEngine, SearchContext, parse_info},
    evaluation::Evaluation,
    fetch::{self, ApiGame},
    review::{Finding, legal_move},
};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, EnPassantMode, Position, fen::Fen};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct SearchRequest {
    pub fen: String,
    pub position_command: String,
    pub nodes: u64,
}
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub fen: String,
    pub position_command: String,
    pub nodes: u64,
    pub info: String,
    pub final_best: String,
}
#[derive(Serialize)]
pub struct Step {
    pub requests: Vec<SearchRequest>,
    pub findings: Vec<Finding>,
    pub positions: usize,
}
pub struct BrowserReview {
    game: CompletedGame,
    user: Color,
    cache: BTreeMap<(String, u64), Analysis>,
    pending: Vec<SearchRequest>,
}
impl BrowserReview {
    pub fn new(game: &ApiGame, username: &str, now: i64) -> Result<Self, String> {
        let username = fetch::username(username).map_err(|e| e.to_string())?;
        if !game.rated || !matches!(game.time_class.as_str(), "rapid" | "blitz" | "bullet") {
            return Err("only rated rapid, blitz and bullet games can be reviewed".into());
        }
        let completed = fetch::verify(game, now)?;
        let user = if game.white.username.eq_ignore_ascii_case(&username) {
            Color::White
        } else if game.black.username.eq_ignore_ascii_case(&username) {
            Color::Black
        } else {
            return Err("reviewed user is not a player".into());
        };
        Ok(Self {
            game: completed,
            user,
            cache: BTreeMap::new(),
            pending: vec![],
        })
    }
    pub fn step(&mut self, responses: Vec<SearchResponse>) -> Result<Step, String> {
        // Validate a complete response set before changing the cache.
        if responses.len() != self.pending.len() {
            return Err("incomplete search response set".into());
        }
        let mut validated = BTreeMap::new();
        for response in responses {
            let key = (response.position_command.clone(), response.nodes);
            if !self.pending.iter().any(|r| {
                r.fen == response.fen
                    && r.nodes == response.nodes
                    && r.position_command == response.position_command
            }) || validated.contains_key(&key)
            {
                return Err("unexpected or duplicate search response".into());
            }
            let position: Chess = response
                .fen
                .parse::<Fen>()
                .map_err(|e| e.to_string())?
                .into_position(shakmaty::CastlingMode::Standard)
                .map_err(|e| e.to_string())?;
            let (evaluation, pv) = parse_info(&response.info, position.turn() == self.user)
                .map_err(|e| e.to_string())?
                .ok_or("no exact score with PV")?;
            let best_uci = pv.first().ok_or("empty engine PV")?.clone();
            legal_move(&position, &response.final_best)?;
            let mut replay = position;
            for uci in &pv {
                let mv = legal_move(&replay, uci)?;
                replay.play_unchecked(mv);
            }
            validated.insert(
                key,
                Analysis {
                    evaluation,
                    best_uci,
                    final_best_uci: response.final_best,
                    pv,
                },
            );
        }
        self.cache.extend(validated);
        let mut engine = CachedEngine {
            cache: &self.cache,
            missing: BTreeMap::new(),
        };
        // Use the exact same budgets and decision rules as the native CLI.
        let mut findings = analysis::findings(
            &self.game,
            self.user,
            &mut engine,
            AnalysisConfig::default(),
        )
        .map_err(|e| e.to_string())?;
        self.pending = engine.missing.into_values().collect();
        if !self.pending.is_empty() {
            findings.clear();
        }
        for finding in &mut findings {
            finding.clock_secs = self
                .game
                .clocks_ms()
                .get(finding.ply as usize - 1)
                .copied()
                .flatten()
                .map(|ms| ms as f64 / 1000.0);
        }
        Ok(Step {
            requests: self.pending.clone(),
            findings,
            positions: self.cache.len(),
        })
    }
}
struct CachedEngine<'a> {
    cache: &'a BTreeMap<(String, u64), Analysis>,
    missing: BTreeMap<(String, u64), SearchRequest>,
}
impl PositionEngine for CachedEngine<'_> {
    fn analyse(
        &mut self,
        position: &Chess,
        user: Color,
        nodes: Nodes,
    ) -> Result<Analysis, EngineError> {
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
        _: Color,
        nodes: Nodes,
    ) -> Result<Analysis, EngineError> {
        let position = context.position;
        let fen = Fen::from_position(position, EnPassantMode::Legal).to_string();
        let position_command = context.position_command();
        let key = (position_command.clone(), nodes.get());
        if let Some(analysis) = self.cache.get(&key) {
            return Ok(analysis.clone());
        }
        self.missing.insert(
            key,
            SearchRequest {
                fen,
                position_command,
                nodes: nodes.get(),
            },
        );
        let mv = position
            .legal_moves()
            .first()
            .ok_or(EngineError::Terminal)?
            .to_uci(shakmaty::CastlingMode::Standard)
            .to_string();
        Ok(Analysis {
            evaluation: Evaluation::centipawns(0),
            best_uci: mv.clone(),
            final_best_uci: mv.clone(),
            pv: vec![mv],
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use std::cell::RefCell;
    thread_local! { static REVIEW: RefCell<Option<BrowserReview>> = const { RefCell::new(None) }; }
    #[derive(Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase")]
    enum Command {
        Start {
            game: ApiGame,
            username: String,
            now: i64,
        },
        Continue {
            responses: Vec<SearchResponse>,
        },
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn review_alloc(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn review_free(ptr: *mut u8, len: usize) {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
        }
    }
    /// Input memory belongs to the caller. The returned u64 packs output length
    /// in the high bits and a separately owned output pointer in the low bits.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn review_step(ptr: *const u8, len: usize) -> u64 {
        let result = (|| -> Result<Step, String> {
            if len > 8_000_000 {
                return Err("browser request is too large".into());
            }
            let input = unsafe { std::slice::from_raw_parts(ptr, len) };
            let command: Command = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            REVIEW.with(|state| {
                let mut state = state.borrow_mut();
                match command {
                    Command::Start {
                        game,
                        username,
                        now,
                    } => {
                        let mut next = BrowserReview::new(&game, &username, now)?;
                        let result = next.step(vec![])?;
                        *state = Some(next);
                        Ok(result)
                    }
                    Command::Continue { responses } => {
                        state.as_mut().ok_or("no review started")?.step(responses)
                    }
                }
            })
        })();
        let bytes = match result {
            Ok(step) => serde_json::to_vec(&step).unwrap(),
            Err(error) => serde_json::to_vec(&serde_json::json!({"error":error})).unwrap(),
        }
        .into_boxed_slice();
        let len = bytes.len();
        let ptr = Box::into_raw(bytes) as *mut u8;
        ((len as u64) << 32) | ptr as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ApiGame {
        serde_json::from_str::<crate::fetch::Month>(include_str!("../tests/fixtures/rapid-12.json"))
            .unwrap()
            .games
            .remove(0)
    }
    #[test]
    fn rejects_ongoing_or_unrelated_input() {
        let mut game = fixture();
        game.end_time = i64::MAX;
        assert!(BrowserReview::new(&game, "dre4success007", 1_800_000_000).is_err());
        assert!(BrowserReview::new(&fixture(), "someoneelse", 1_800_000_000).is_err());
    }
    #[test]
    fn never_emits_findings_before_all_searches_are_present() {
        let mut review = BrowserReview::new(&fixture(), "dre4success007", 1_800_000_000).unwrap();
        let first = review.step(vec![]).unwrap();
        assert!(!first.requests.is_empty());
        assert!(first.findings.is_empty());
        assert!(first.requests.iter().all(|r| r.nodes == 150_000));
        assert!(
            first
                .requests
                .iter()
                .any(|r| r.position_command.contains(" moves "))
        );
        assert!(review.step(vec![]).is_err());
    }
    #[test]
    fn cache_distinguishes_histories_that_reach_the_same_fen() {
        let initial = Chess::default();
        let sequences: Vec<Vec<shakmaty::uci::UciMove>> =
            ["g1f3 g8f6 b1c3 b8c6", "b1c3 b8c6 g1f3 g8f6"]
                .into_iter()
                .map(|s| s.split_whitespace().map(|m| m.parse().unwrap()).collect())
                .collect();
        let positions: Vec<_> = sequences
            .iter()
            .map(|moves| {
                let mut p = initial.clone();
                for mv in moves {
                    p.play_unchecked(mv.to_move(&p).unwrap());
                }
                p
            })
            .collect();
        assert_eq!(positions[0], positions[1]);
        let cache = BTreeMap::new();
        let mut engine = CachedEngine {
            cache: &cache,
            missing: BTreeMap::new(),
        };
        for (position, moves) in positions.iter().zip(&sequences) {
            engine
                .analyse_with_history(
                    SearchContext {
                        position,
                        initial: &initial,
                        moves,
                    },
                    Color::White,
                    Nodes::new(150_000).unwrap(),
                )
                .unwrap();
        }
        assert_eq!(engine.missing.len(), 2);
    }
    #[test]
    fn cached_browser_adapter_matches_native_pipeline() {
        struct Flat;
        impl PositionEngine for Flat {
            fn analyse(&mut self, p: &Chess, _: Color, _: Nodes) -> Result<Analysis, EngineError> {
                let mv = p.legal_moves()[0]
                    .to_uci(shakmaty::CastlingMode::Standard)
                    .to_string();
                Ok(Analysis {
                    evaluation: Evaluation::centipawns(0),
                    best_uci: mv.clone(),
                    final_best_uci: mv.clone(),
                    pv: vec![mv],
                })
            }
        }
        let mut review = BrowserReview::new(&fixture(), "dre4success007", 1_800_000_000).unwrap();
        let native = analysis::findings(
            &review.game,
            review.user,
            &mut Flat,
            AnalysisConfig::default(),
        )
        .unwrap();
        let mut step = review.step(vec![]).unwrap();
        while !step.requests.is_empty() {
            let responses = step
                .requests
                .into_iter()
                .map(|r| {
                    let p: Chess = r
                        .fen
                        .parse::<Fen>()
                        .unwrap()
                        .into_position(shakmaty::CastlingMode::Standard)
                        .unwrap();
                    let mv = p.legal_moves()[0]
                        .to_uci(shakmaty::CastlingMode::Standard)
                        .to_string();
                    SearchResponse {
                        fen: r.fen,
                        position_command: r.position_command,
                        nodes: r.nodes,
                        info: format!("info score cp 0 pv {mv}"),
                        final_best: mv,
                    }
                })
                .collect();
            step = review.step(responses).unwrap();
        }
        assert_eq!(native, step.findings);
    }
}
