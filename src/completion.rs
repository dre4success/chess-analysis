use std::collections::BTreeMap;

use crate::pgn::RawGame;
use pgn_reader::{KnownOutcome, Outcome};
use shakmaty::{Chess, uci::UciMove};
use thiserror::Error;

pub trait CompletionPolicy {
    fn verify(&self, game: RawGame) -> Result<CompletedGame, RejectionReason>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalPgnPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedGame {
    game: RawGame,
    outcome: KnownOutcome,
}

impl CompletedGame {
    #[must_use]
    pub fn headers(&self) -> &BTreeMap<String, String> {
        self.game.headers()
    }

    #[must_use]
    pub fn moves(&self) -> &[UciMove] {
        self.game.moves()
    }

    #[must_use]
    pub const fn outcome(&self) -> KnownOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn initial_position(&self) -> &Chess {
        self.game.initial_position()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RejectionReason {
    #[error("game has no moves")]
    NoMoves,

    #[error("Result header is missing")]
    MissingHeaderResult,

    #[error("Result header is invalid: {0}")]
    InvalidHeaderResult(String),

    #[error("Result header is ongoing or unknown")]
    UnknownHeaderResult,

    #[error("movetext result marker is missing")]
    MissingMovetextResult,

    #[error("movetext result marker is ongoing or unknown")]
    UnknownMovetextResult,

    #[error("Result header {header} does not match movetext result {movetext}")]
    MismatchedResult {
        header: KnownOutcome,
        movetext: KnownOutcome,
    },

    #[error("unsupported chess variant: {0}")]
    UnsupportedVariant(String),
}

impl CompletionPolicy for LocalPgnPolicy {
    fn verify(&self, game: RawGame) -> Result<CompletedGame, RejectionReason> {
        if game.moves().is_empty() {
            return Err(RejectionReason::NoMoves);
        }

        if let Some(variant) = game.headers().get("Variant")
            && !variant.eq_ignore_ascii_case("standard")
            && !variant.eq_ignore_ascii_case("chess")
        {
            return Err(RejectionReason::UnsupportedVariant(variant.clone()));
        }

        let header_text = game
            .headers()
            .get("Result")
            .ok_or(RejectionReason::MissingHeaderResult)?;

        let header_outcome = Outcome::from_ascii(header_text.as_bytes())
            .map_err(|_| RejectionReason::InvalidHeaderResult(header_text.clone()))?;

        let header_known = header_outcome
            .known()
            .ok_or(RejectionReason::UnknownHeaderResult)?;

        let movetext_outcome = game
            .movetext_outcome()
            .ok_or(RejectionReason::MissingMovetextResult)?;

        let movetext_known = movetext_outcome
            .known()
            .ok_or(RejectionReason::UnknownMovetextResult)?;

        if header_known != movetext_known {
            return Err(RejectionReason::MismatchedResult {
                header: header_known,
                movetext: movetext_known,
            });
        }

        Ok(CompletedGame {
            game,
            outcome: header_known,
        })
    }
}

#[cfg(test)]
mod tests {
    use pgn_reader::KnownOutcome;
    use shakmaty::Color;

    use super::{CompletionPolicy, LocalPgnPolicy};
    use crate::{
        completion::RejectionReason,
        pgn::{RawGame, parse_one},
    };

    fn parse(pgn: &str) -> RawGame {
        parse_one(pgn.as_bytes())
            .expect("PGN should be readable")
            .expect("PGN should contain one game")
    }

    #[test]
    fn accepts_finished_local_pgn_without_termination_header() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]

  1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0
  "#,
        );

        let completed = LocalPgnPolicy
            .verify(game)
            .expect("matching finished results should be accepted");

        assert_eq!(
            completed.outcome(),
            KnownOutcome::Decisive {
                winner: Color::White
            }
        );
        assert_eq!(completed.moves().len(), 7);
    }

    #[test]
    fn rejects_unknown_result_header() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "*"]

  1. e4 *
  "#,
        );
        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::UnknownHeaderResult)
        );
    }

    #[test]
    fn rejects_invalid_result_header() {
        let game = parse(
            r#"
[White "Alice"]
[Black "Bob"]
[Result "victory"]

1. e4 1-0
"#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::InvalidHeaderResult("victory".to_owned()))
        );
    }

    #[test]
    fn rejects_unknown_movetext_result_marker() {
        let game = parse(
            r#"
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 *
"#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::UnknownMovetextResult)
        );
    }

    #[test]
    fn rejects_missing_result_header() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]

  1. e4 1-0
  "#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::MissingHeaderResult)
        );
    }

    #[test]
    fn rejects_missing_movetext_result_marker() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]

  1. e4
  "#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::MissingMovetextResult)
        );
    }

    #[test]
    fn rejects_mismatched_results() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]

  1. e4 0-1
  "#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::MismatchedResult {
                header: KnownOutcome::Decisive {
                    winner: Color::White
                },
                movetext: KnownOutcome::Decisive {
                    winner: Color::Black
                },
            })
        );
    }

    #[test]
    fn rejects_non_standard_variant() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]
  [Variant "Chess960"]

  1. e4 1-0
  "#,
        );

        assert_eq!(
            LocalPgnPolicy.verify(game),
            Err(RejectionReason::UnsupportedVariant("Chess960".to_owned()))
        );
    }

    #[test]
    fn rejects_game_without_moves() {
        let game = parse(
            r#"
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]

  1-0
  "#,
        );

        assert_eq!(LocalPgnPolicy.verify(game), Err(RejectionReason::NoMoves));
    }
}
