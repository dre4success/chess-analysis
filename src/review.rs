use pgn_reader::SanPlus;
use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, uci::UciMove};

use crate::evaluation::Evaluation;
use thiserror::Error;

fn default_threshold() -> u32 {
    200
}

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewMode {
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colour {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameResult {
    Win,
    Loss,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Verified,
    Structural,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    LineOpened,
    LineOnto,
    DefenderLeft,
    AttackedPieceIgnored,
    CaptureCost,
    MissedMate,
    EngineVerifiedMistake,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineMetadata {
    pub name: String,
    pub version: String,
    pub executable_sha256: String,
    pub nnue_sha256: Option<String>,
    #[serde(default)]
    pub nnue_identity: String,
    #[serde(default)]
    pub uci_options: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_threshold")]
    pub threshold_cp: u32,
    pub scan_nodes: u64,
    pub deep_nodes: u64,
    pub threads: u16,
    pub hash_mb: u32,
    pub multipv: u16,
    pub clear_hash_between_positions: bool,
    pub analysis_order: String,
    pub platform: String,
    pub architecture: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub schema_version: String,
    pub user: String,
    pub generated: String,
    pub mode: ReviewMode,
    pub engine: EngineMetadata,
    pub games: Vec<GameReview>,
    pub patterns: Vec<Pattern>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedReview(Review);
impl ValidatedReview {
    #[must_use]
    pub const fn as_review(&self) -> &Review {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> Review {
        self.0
    }
}
#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("invalid review: {0}")]
    Integrity(String),
    #[error("unsupported review schema version `{found}`; expected `{expected}`")]
    UnsupportedSchemaVersion {
        expected: &'static str,
        found: String,
    },

    #[error("game {game}, finding {finding} has invalid before_fen: {reason}")]
    InvalidBeforeFen {
        game: usize,
        finding: usize,
        reason: String,
    },

    #[error("game {game}, finding {finding} has invalid after_fen: {reason}")]
    InvalidAfterFen {
        game: usize,
        finding: usize,
        reason: String,
    },

    #[error("game {game}, finding {finding} has illegal actual move `{uci}`: {reason}")]
    IllegalActualMove {
        game: usize,
        finding: usize,
        uci: String,
        reason: String,
    },

    #[error(
        "game {game}, finding {finding} after_fen mismatch: expected `{expected}`, found `{found}`"
    )]
    AfterFenMismatch {
        game: usize,
        finding: usize,
        expected: String,
        found: String,
    },
    #[error(
        "game {game}, finding {finding} actual SAN mismatch: expected `{expected}`, found `{found}`"
    )]
    ActualSanMismatch {
        game: usize,
        finding: usize,
        expected: String,
        found: String,
    },

    #[error("game {game}, finding {finding} must store best_uci and best_san together")]
    IncompleteBestMove { game: usize, finding: usize },

    #[error("game {game}, finding {finding} has illegal best move `{uci}`: {reason}")]
    IllegalBestMove {
        game: usize,
        finding: usize,
        uci: String,
        reason: String,
    },

    #[error(
        "game {game}, finding {finding} best SAN mismatch: expected `{expected}`, found `{found}`"
    )]
    BestSanMismatch {
        game: usize,
        finding: usize,
        expected: String,
        found: String,
    },

    #[error("game {game}, finding {finding} has illegal PV move {pv_index} `{uci}`: {reason}")]
    IllegalPvMove {
        game: usize,
        finding: usize,
        pv_index: usize,
        uci: String,
        reason: String,
    },
}

pub fn validate(review: Review) -> Result<ValidatedReview, ValidationError> {
    if review.schema_version != SCHEMA_VERSION {
        return Err(ValidationError::UnsupportedSchemaVersion {
            expected: SCHEMA_VERSION,
            found: review.schema_version,
        });
    }

    let mut urls = std::collections::BTreeSet::new();
    for (game_index, game) in review.games.iter().enumerate() {
        if !urls.insert(&game.url) {
            return Err(ValidationError::Integrity(format!(
                "duplicate game URL {}",
                game.url
            )));
        }
        if game
            .clock_used_pct
            .is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v))
        {
            return Err(ValidationError::Integrity(format!(
                "invalid clock percentage in game {game_index}"
            )));
        }
        let mut plies = std::collections::BTreeSet::new();
        for f in &game.findings {
            if f.ply == 0 || !plies.insert(f.ply) {
                return Err(ValidationError::Integrity(format!(
                    "zero or duplicate ply in game {game_index}"
                )));
            }
            if f.clock_secs.is_some_and(|v| !v.is_finite() || v < 0.0) {
                return Err(ValidationError::Integrity("invalid clock seconds".into()));
            }
            if f.explanation.trim().is_empty() {
                return Err(ValidationError::Integrity("empty explanation".into()));
            }
        }
        for (finding_index, finding) in game.findings.iter().enumerate() {
            validate_finding(finding, game.colour, game_index, finding_index)?;
        }
    }

    let mut classes = std::collections::BTreeSet::new();
    for p in &review.patterns {
        if !classes.insert(p.classification) {
            return Err(ValidationError::Integrity("duplicate pattern".into()));
        }
        let affected = review
            .games
            .iter()
            .filter(|g| {
                g.findings
                    .iter()
                    .any(|f| f.classification == p.classification)
            })
            .count();
        let occurrences = review
            .games
            .iter()
            .flat_map(|g| &g.findings)
            .filter(|f| f.classification == p.classification)
            .count();
        if affected != p.games_affected as usize || occurrences != p.occurrences as usize {
            return Err(ValidationError::Integrity(
                "pattern counts do not match findings".into(),
            ));
        }
        for e in &p.example_refs {
            if !review.games.iter().any(|g| {
                g.url == e.url
                    && g.findings
                        .iter()
                        .any(|f| f.ply == e.ply && f.classification == p.classification)
            }) {
                return Err(ValidationError::Integrity(
                    "unresolved pattern example".into(),
                ));
            }
        }
    }
    let expected_classes: std::collections::BTreeSet<_> = review
        .games
        .iter()
        .flat_map(|g| g.findings.iter().map(|f| f.classification))
        .collect();
    if classes != expected_classes {
        return Err(ValidationError::Integrity(
            "findings are missing from pattern summary".into(),
        ));
    }
    Ok(ValidatedReview(review))
}

fn parse_position(fen_text: &str) -> Result<Chess, String> {
    let fen = Fen::from_ascii(fen_text.as_bytes()).map_err(|error| error.to_string())?;

    fen.into_position(CastlingMode::Standard)
        .map_err(|error| error.to_string())
}

fn validate_finding(
    finding: &Finding,
    colour: Colour,
    game_index: usize,
    finding_index: usize,
) -> Result<(), ValidationError> {
    let before_position = parse_position(&finding.before_fen).map_err(|reason| {
        ValidationError::InvalidBeforeFen {
            game: game_index,
            finding: finding_index,
            reason,
        }
    })?;

    let expected_turn = match colour {
        Colour::White => shakmaty::Color::White,
        Colour::Black => shakmaty::Color::Black,
    };
    if before_position.turn() != expected_turn {
        return Err(ValidationError::Integrity(format!(
            "game {game_index}, finding {finding_index}: move belongs to the other colour"
        )));
    }
    let mut position = before_position.clone();

    parse_position(&finding.after_fen).map_err(|reason| ValidationError::InvalidAfterFen {
        game: game_index,
        finding: finding_index,
        reason,
    })?;

    let uci = UciMove::from_ascii(finding.actual_uci.as_bytes()).map_err(|error| {
        ValidationError::IllegalActualMove {
            game: game_index,
            finding: finding_index,
            uci: finding.actual_uci.clone(),
            reason: error.to_string(),
        }
    })?;

    let chess_move =
        uci.to_move(&position)
            .map_err(|error| ValidationError::IllegalActualMove {
                game: game_index,
                finding: finding_index,
                uci: finding.actual_uci.clone(),
                reason: error.to_string(),
            })?;

    let expected_actual_san = SanPlus::from_move(before_position.clone(), chess_move).to_string();

    if expected_actual_san != finding.actual_san {
        return Err(ValidationError::ActualSanMismatch {
            game: game_index,
            finding: finding_index,
            expected: expected_actual_san,
            found: finding.actual_san.clone(),
        });
    }

    position.play_unchecked(chess_move);

    let expected_after = Fen::from_position(&position, EnPassantMode::Legal).to_string();

    if expected_after != finding.after_fen {
        return Err(ValidationError::AfterFenMismatch {
            game: game_index,
            finding: finding_index,
            expected: expected_after,
            found: finding.after_fen.clone(),
        });
    }

    match (&finding.best_uci, &finding.best_san) {
        (None, None) => {}
        (Some(uci), Some(san)) => {
            let mv = legal_move(&before_position, uci).map_err(|reason| {
                ValidationError::IllegalBestMove {
                    game: game_index,
                    finding: finding_index,
                    uci: uci.clone(),
                    reason,
                }
            })?;
            let expected = SanPlus::from_move(before_position.clone(), mv).to_string();
            if expected != *san {
                return Err(ValidationError::BestSanMismatch {
                    game: game_index,
                    finding: finding_index,
                    expected,
                    found: san.clone(),
                });
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(ValidationError::IncompleteBestMove {
                game: game_index,
                finding: finding_index,
            });
        }
    }

    if let (Some(best), Some(first)) = (&finding.best_uci, finding.principal_variation_uci.first())
        && best != first
    {
        return Err(ValidationError::Integrity(format!(
            "game {game_index}, finding {finding_index}: PV does not begin with best_uci"
        )));
    }
    let mut pv_position = before_position;
    for (pv_index, uci) in finding.principal_variation_uci.iter().enumerate() {
        let mv =
            legal_move(&pv_position, uci).map_err(|reason| ValidationError::IllegalPvMove {
                game: game_index,
                finding: finding_index,
                pv_index,
                uci: uci.clone(),
                reason,
            })?;
        pv_position.play_unchecked(mv);
    }
    Ok(())
}

pub(crate) fn legal_move(position: &Chess, uci: &str) -> Result<shakmaty::Move, String> {
    UciMove::from_ascii(uci.as_bytes())
        .map_err(|e| e.to_string())?
        .to_move(position)
        .map_err(|e| e.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewIoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: &std::path::Path) -> Result<ValidatedReview, ReviewIoError> {
    Ok(validate(serde_json::from_reader(std::fs::File::open(
        path,
    )?)?)?)
}

/// Only validated, immutable reviews can cross the persistence boundary.
#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: &std::path::Path, review: &ValidatedReview) -> Result<(), ReviewIoError> {
    let bytes = serde_json::to_vec_pretty(review.as_review())?;
    atomic_write(path, &bytes)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameReview {
    pub url: String,
    pub input_pgn_sha256: String,
    pub date: String,
    pub colour: Colour,
    pub result: GameResult,
    pub opponent: Opponent,
    pub time_control: String,
    pub clock_used_pct: Option<f64>,
    pub eco: Option<String>,
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub phase_coverage: Vec<String>,
    #[serde(default)]
    pub clock_band_coverage: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opponent {
    pub name: String,
    pub rating: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub ply: u32,
    pub before_fen: String,
    pub after_fen: String,
    pub actual_uci: String,
    pub actual_san: String,
    pub best_uci: Option<String>,
    pub best_san: Option<String>,
    pub eval_before: Evaluation,
    pub eval_after: Evaluation,
    pub classification: Classification,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_matched: Vec<Classification>,

    pub explanation: String,
    pub principal_variation_uci: Vec<String>,
    pub confidence: Confidence,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_secs: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub classification: Classification,
    pub games_affected: u32,
    pub occurrences: u32,
    #[serde(default)]
    pub games_reviewed: u32,
    #[serde(default)]
    pub breakdowns: Vec<Breakdown>,
    pub example_refs: Vec<ExampleRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breakdown {
    pub dimension: String,
    pub value: String,
    pub games_affected: u32,
    pub games_reviewed: u32,
    pub occurrences: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleRef {
    pub url: String,
    pub ply: u32,
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{Value, json};

    use super::{
        Classification, Colour, Confidence, EngineMetadata, ExampleRef, Finding, GameResult,
        GameReview, Opponent, Pattern, Review, ReviewMode, SCHEMA_VERSION, ValidationError,
        validate,
    };
    use crate::evaluation::{Evaluation, ReviewSide};

    pub(crate) fn sample_review() -> Review {
        let game_url = "https://www.chess.com/game/live/172386685690";

        Review {
            schema_version: SCHEMA_VERSION.to_owned(),
            user: "dre4success007".to_owned(),
            generated: "2026-08-01T18:04:00Z".to_owned(),
            mode: ReviewMode::Verified,
            engine: EngineMetadata {
                name: "Stockfish".to_owned(),
                version: "17".to_owned(),
                executable_sha256: "engine-sha256".to_owned(),
                nnue_sha256: Some("nnue-sha256".to_owned()),
                nnue_identity: "embedded".into(),
                uci_options: Default::default(),
                threshold_cp: 200,
                scan_nodes: 150_000,
                deep_nodes: 1_000_000,
                threads: 1,
                hash_mb: 64,
                multipv: 1,
                clear_hash_between_positions: true,
                analysis_order: "game-order".to_owned(),
                platform: "macos".to_owned(),
                architecture: "aarch64".to_owned(),
            },
            games: vec![GameReview {
                url: game_url.to_owned(),
                input_pgn_sha256: "pgn-sha256".to_owned(),
                date: "2026-08-01".to_owned(),
                colour: Colour::Black,
                result: GameResult::Loss,
                opponent: Opponent {
                    name: "Anil4766".to_owned(),
                    rating: Some(819),
                },
                time_control: "600".to_owned(),
                clock_used_pct: Some(36.0),
                eco: Some("B10".to_owned()),
                phase_coverage: vec![],
                clock_band_coverage: vec![],
                findings: vec![Finding {
                    ply: 30,
                    before_fen: "r1bqr1k1/pp3pp1/5n1p/3p4/1b1P3B/1B1Q3P/PP3PP1/R2K2NR b - - 2 15"
                        .to_owned(),
                    after_fen: "r1bqr1k1/pp3pp1/7p/3p4/1b1Pn2B/1B1Q3P/PP3PP1/R2K2NR w - - 3 16"
                        .to_owned(),
                    actual_uci: "f6e4".to_owned(),
                    actual_san: "Ne4".to_owned(),
                    best_uci: Some("c8e6".to_owned()),
                    best_san: Some("Be6".to_owned()),
                    eval_before: Evaluation::centipawns(20),
                    eval_after: Evaluation::centipawns(-610),
                    classification: Classification::LineOpened,
                    also_matched: vec![Classification::DefenderLeft],
                    explanation: "Ne4 opened the bishop line onto your queen.".to_owned(),
                    principal_variation_uci: vec![
                        "c8e6".to_owned(),
                        "h4g3".to_owned(),
                        "d8d7".to_owned(),
                    ],
                    confidence: Confidence::Verified,
                    clock_secs: None,
                }],
            }],
            patterns: vec![Pattern {
                classification: Classification::LineOpened,
                games_affected: 1,
                occurrences: 1,
                games_reviewed: 1,
                breakdowns: vec![],
                example_refs: vec![ExampleRef {
                    url: game_url.to_owned(),
                    ply: 30,
                }],
            }],
        }
    }

    #[test]
    fn complete_review_round_trips_through_json() {
        let review = sample_review();
        let encoded = serde_json::to_string_pretty(&review).expect("review should serialize");
        let decoded: Review = serde_json::from_str(&encoded).expect("review should deserialize");

        assert_eq!(decoded, review);
    }

    #[test]
    fn complete_review_uses_the_canonical_wire_names() {
        let value = serde_json::to_value(sample_review()).expect("review should serialize");
        let game = &value["games"][0];
        let finding = &game["findings"][0];

        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert_eq!(value["mode"], "verified");
        assert_eq!(game["result"], "loss");
        assert_eq!(finding["classification"], "line-opened");
        assert_eq!(finding["also_matched"], json!(["defender-left"]));
        assert_eq!(
            finding["eval_before"],
            json!({"type": "cp", "value": 20, "pov": "user"})
        );
        assert!(finding.get("clock_secs").is_none());
    }

    #[test]
    fn empty_secondary_classifications_are_omitted() {
        let mut review = sample_review();
        review.games[0].findings[0].also_matched.clear();

        let value = serde_json::to_value(review).expect("review should serialize");
        let finding = &value["games"][0]["findings"][0];

        assert!(finding.get("also_matched").is_none());
    }

    #[test]
    fn every_classification_has_its_contract_name() {
        let cases = [
            (Classification::LineOpened, "line-opened"),
            (Classification::LineOnto, "line-onto"),
            (Classification::DefenderLeft, "defender-left"),
            (
                Classification::AttackedPieceIgnored,
                "attacked-piece-ignored",
            ),
            (Classification::CaptureCost, "capture-cost"),
            (Classification::MissedMate, "missed-mate"),
            (
                Classification::EngineVerifiedMistake,
                "engine-verified-mistake",
            ),
        ];

        for (classification, expected) in cases {
            let value = serde_json::to_value(classification)
                .expect("classification should serialize to JSON");
            assert_eq!(value, Value::String(expected.to_owned()));
        }
    }

    #[test]
    fn mate_evaluation_remains_structured_inside_a_review() {
        let mut review = sample_review();
        review.games[0].findings[0].eval_after = Evaluation::mate(ReviewSide::Opponent, 2);

        let value = serde_json::to_value(review).expect("review should serialize");

        assert_eq!(
            value["games"][0]["findings"][0]["eval_after"],
            json!({
                "type": "mate",
                "winner": "opponent",
                "moves": 2,
                "pov": "user"
            })
        );
    }

    #[test]
    fn accepts_the_current_schema_version() {
        let review = sample_review();

        let validated = validate(review.clone()).expect("current schema should validate");

        assert_eq!(validated.as_review(), &review);
        assert_eq!(validated.into_inner(), review);
    }

    #[test]
    fn rejects_an_unsupported_schema_version() {
        let mut review = sample_review();
        review.schema_version = "2.0".to_owned();

        assert_eq!(
            validate(review),
            Err(ValidationError::UnsupportedSchemaVersion {
                expected: SCHEMA_VERSION,
                found: "2.0".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_an_invalid_before_fen() {
        let mut review = sample_review();
        review.games[0].findings[0].before_fen = "not a fen".to_owned();

        assert!(matches!(
            validate(review),
            Err(ValidationError::InvalidBeforeFen {
                game: 0,
                finding: 0,
                ..
            })
        ));
    }

    #[test]
    fn rejects_an_invalid_after_fen() {
        let mut review = sample_review();
        review.games[0].findings[0].after_fen = "not a fen".to_owned();

        assert!(matches!(
            validate(review),
            Err(ValidationError::InvalidAfterFen {
                game: 0,
                finding: 0,
                ..
            })
        ));
    }

    #[test]
    fn rejects_an_illegal_actual_move() {
        let mut review = sample_review();
        review.games[0].findings[0].actual_uci = "e2e4".to_owned();

        assert!(matches!(
            validate(review),
            Err(ValidationError::IllegalActualMove {
                game: 0,
                finding: 0,
                ..
            })
        ));
    }

    #[test]
    fn rejects_an_after_fen_that_does_not_follow_the_actual_move() {
        let mut review = sample_review();
        review.games[0].findings[0].after_fen =
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned();

        assert!(matches!(
            validate(review),
            Err(ValidationError::AfterFenMismatch {
                game: 0,
                finding: 0,
                ..
            })
        ));
    }
    #[test]
    fn validates_best_move_san_and_sequential_pv() {
        let mut r = sample_review();
        r.games[0].findings[0].best_uci = Some("e2e4".into());
        assert!(matches!(
            validate(r),
            Err(ValidationError::IllegalBestMove { .. })
        ));
        let mut r = sample_review();
        r.games[0].findings[0].best_san = Some("Qe6".into());
        assert!(matches!(
            validate(r),
            Err(ValidationError::BestSanMismatch { .. })
        ));
        let mut r = sample_review();
        r.games[0].findings[0].principal_variation_uci[1] = "c8e6".into();
        assert!(matches!(
            validate(r),
            Err(ValidationError::IllegalPvMove { pv_index: 1, .. })
        ));
    }

    #[test]
    fn required_fields_cannot_be_imported_as_missing() {
        for field in ["classification", "before_fen", "after_fen"] {
            let mut value = serde_json::to_value(sample_review()).unwrap();
            value["games"][0]["findings"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<Review>(value).is_err());
        }
    }

    #[test]
    fn atomic_round_trip_and_failed_write_preserve_existing_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("review.json");
        let validated = validate(sample_review()).unwrap();
        super::write(&path, &validated).unwrap();
        assert_eq!(super::read(&path).unwrap(), validated);
        assert!(super::write(&dir.path().join("missing/review.json"), &validated).is_err());
        assert_eq!(super::read(&path).unwrap(), validated);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn rejects_nonfinite_clocks_and_dangling_or_missing_patterns() {
        let mut r = sample_review();
        r.games[0].clock_used_pct = Some(f64::NAN);
        assert!(matches!(validate(r), Err(ValidationError::Integrity(_))));
        let mut r = sample_review();
        r.patterns[0].example_refs[0].ply = 100;
        assert!(matches!(validate(r), Err(ValidationError::Integrity(_))));
        let mut r = sample_review();
        r.patterns.clear();
        assert!(matches!(validate(r), Err(ValidationError::Integrity(_))));
    }
    #[test]
    fn validates_special_moves_by_replaying_canonical_fens_and_san() {
        use shakmaty::{EnPassantMode, Position, fen::Fen, san::SanPlus};
        for (fen, uci) in [
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1g1"),
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1c1"),
            ("7k/8/8/3pP3/8/8/8/7K w - d6 0 1", "e5d6"),
            ("7k/P7/8/8/8/8/8/7K w - - 0 1", "a7a8q"),
            ("7k/P7/8/8/8/8/8/7K w - - 0 1", "a7a8n"),
        ] {
            let before = super::parse_position(fen).unwrap();
            let mv = super::legal_move(&before, uci).unwrap();
            let after = before.clone().play(mv).unwrap();
            let mut r = sample_review();
            r.games[0].colour = Colour::White;
            let f = &mut r.games[0].findings[0];
            f.before_fen = fen.into();
            f.after_fen = Fen::from_position(&after, EnPassantMode::Legal).to_string();
            f.actual_uci = uci.into();
            f.actual_san = SanPlus::from_move(before, mv).to_string();
            f.best_uci = Some(uci.into());
            f.best_san = Some(f.actual_san.clone());
            f.principal_variation_uci = vec![uci.into()];
            validate(r).unwrap();
        }
    }
}
