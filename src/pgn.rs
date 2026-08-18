use pgn_reader::{Outcome, Reader, SanPlus, Visitor};
use shakmaty::{CastlingMode, Chess, Position, fen::Fen, uci::UciMove};
use std::{
    collections::BTreeMap,
    io::{self, Cursor},
    ops::ControlFlow,
    str,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGame {
    headers: BTreeMap<String, String>,
    moves: Vec<UciMove>,
    movetext_outcome: Option<Outcome>,
    initial_position: Chess,
}

impl RawGame {
    pub(crate) fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub(crate) fn moves(&self) -> &[UciMove] {
        &self.moves
    }

    pub(crate) fn movetext_outcome(&self) -> Option<Outcome> {
        self.movetext_outcome
    }

    pub(crate) const fn initial_position(&self) -> &Chess {
        &self.initial_position
    }
}

#[derive(Debug, Error)]
pub enum ParseGameError {
    #[error("could not read PGN: {0}")]
    Io(#[from] io::Error),

    #[error("PGN contains invalid UTF-8 in a tag")]
    InvalidTagEncoding,

    #[error("illegal SAN move: {0}")]
    IllegalMove(String),

    #[error("SetUp header requests a custom position but FEN is missing")]
    MissingStartingFen,

    #[error("invalid starting FEN `{fen}`: {reason}")]
    InvalidStartingPosition { fen: String, reason: String },
}

#[derive(Debug)]
struct MovetextState {
    headers: BTreeMap<String, String>,
    position: Chess,
    moves: Vec<UciMove>,
    outcome: Option<Outcome>,
    initial_position: Chess,
}

#[derive(Debug, Default)]
struct RawGameVisitor;

impl Visitor for RawGameVisitor {
    type Tags = BTreeMap<String, String>;
    type Movetext = MovetextState;
    type Output = Result<RawGame, ParseGameError>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(BTreeMap::new())
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: pgn_reader::RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let name = match str::from_utf8(name) {
            Ok(name) => name,
            Err(_) => {
                return ControlFlow::Break(Err(ParseGameError::InvalidTagEncoding));
            }
        };

        let value = match value.decode_utf8() {
            Ok(value) => value,
            Err(_) => {
                return ControlFlow::Break(Err(ParseGameError::InvalidTagEncoding));
            }
        };

        tags.insert(name.to_owned(), value.into_owned());
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, headers: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        let position = match starting_position(&headers) {
            Ok(position) => position,
            Err(error) => return ControlFlow::Break(Err(error)),
        };

        ControlFlow::Continue(MovetextState {
            headers,
            initial_position: position.clone(),
            position,
            moves: Vec::new(),
            outcome: None,
        })
    }

    fn san(
        &mut self,
        movetext: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        let chess_move = match san_plus.san.to_move(&movetext.position) {
            Ok(chess_move) => chess_move,
            Err(error) => {
                return ControlFlow::Break(Err(ParseGameError::IllegalMove(error.to_string())));
            }
        };

        let uci = chess_move.to_uci(CastlingMode::Standard);
        movetext.position.play_unchecked(chess_move);
        movetext.moves.push(uci);

        ControlFlow::Continue(())
    }

    fn outcome(
        &mut self,
        movetext: &mut Self::Movetext,
        outcome: Outcome,
    ) -> ControlFlow<Self::Output> {
        movetext.outcome = Some(outcome);
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        Ok(RawGame {
            headers: movetext.headers,
            moves: movetext.moves,
            movetext_outcome: movetext.outcome,
            initial_position: movetext.initial_position,
        })
    }
}

fn starting_position(headers: &BTreeMap<String, String>) -> Result<Chess, ParseGameError> {
    let Some(fen_text) = headers.get("FEN") else {
        if headers.get("SetUp").is_some_and(|value| value == "1") {
            return Err(ParseGameError::MissingStartingFen);
        }

        return Ok(Chess::default());
    };

    let fen = Fen::from_ascii(fen_text.as_bytes()).map_err(|error| {
        ParseGameError::InvalidStartingPosition {
            fen: fen_text.clone(),
            reason: error.to_string(),
        }
    })?;

    fen.into_position(CastlingMode::Standard).map_err(|error| {
        ParseGameError::InvalidStartingPosition {
            fen: fen_text.clone(),
            reason: error.to_string(),
        }
    })
}

pub fn parse_one(input: &[u8]) -> Result<Option<RawGame>, ParseGameError> {
    let mut reader = Reader::new(Cursor::new(input));
    let parsed = reader.read_game(&mut RawGameVisitor)?;

    parsed.transpose()
}

#[cfg(test)]
mod tests {
    use super::{ParseGameError, parse_one};

    const CHECKMATE_PGN: &[u8] = br#"
  [Event "Parser test"]
  [White "Alice"]
  [Black "Bob"]
  [Result "1-0"]
  [Termination "checkmate"]

  1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0
  "#;

    #[test]
    fn parses_headers_and_replays_legal_moves() {
        let game = parse_one(CHECKMATE_PGN)
            .expect("PGN should be readable")
            .expect("PGN should contain one game");

        assert_eq!(game.headers.get("White").map(String::as_str), Some("Alice"));
        assert_eq!(game.headers.get("Black").map(String::as_str), Some("Bob"));

        let moves = game
            .moves
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        assert_eq!(moves.join(" "), "e2e4 e7e5 f1c4 b8c6 d1h5 g8f6 h5f7");

        assert!(game.movetext_outcome.is_some());
    }

    #[test]
    fn replays_standard_castling_as_canonical_uci() {
        let game = parse_one(
            br#"
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. O-O 1-0
"#,
        )
        .expect("PGN should be readable")
        .expect("PGN should contain one game");

        assert_eq!(
            game.moves.last().map(ToString::to_string).as_deref(),
            Some("e1g1")
        );
    }

    #[test]
    fn replays_queen_promotion_from_fen() {
        let game = parse_one(
            br#"
[SetUp "1"]
[FEN "7k/P7/8/8/8/8/8/7K w - - 0 1"]
[Result "1-0"]

1. a8=Q+ 1-0
"#,
        )
        .expect("FEN-based PGN should be readable")
        .expect("PGN should contain one game");

        assert_eq!(game.moves[0].to_string(), "a7a8q");
    }

    #[test]
    fn replays_underpromotion_from_fen() {
        let game = parse_one(
            br#"
[SetUp "1"]
[FEN "7k/P7/8/8/8/8/8/7K w - - 0 1"]
[Result "1/2-1/2"]

1. a8=N 1/2-1/2
"#,
        )
        .expect("FEN-based PGN should be readable")
        .expect("PGN should contain one game");

        assert_eq!(game.moves[0].to_string(), "a7a8n");
    }

    #[test]
    fn rejects_setup_header_without_fen() {
        let error = parse_one(
            br#"
[SetUp "1"]
[Result "1-0"]

1. e4 1-0
"#,
        )
        .expect_err("SetUp without FEN must be rejected");

        assert!(matches!(error, ParseGameError::MissingStartingFen));
    }

    #[test]
    fn rejects_invalid_starting_fen() {
        let error = parse_one(
            br#"
[SetUp "1"]
[FEN "not a fen"]
[Result "1-0"]

1. e4 1-0
"#,
        )
        .expect_err("invalid FEN must be rejected");

        assert!(matches!(
            error,
            ParseGameError::InvalidStartingPosition { .. }
        ));
    }

    #[test]
    fn rejects_san_that_is_illegal_in_the_current_position() {
        let error = parse_one(
            br#"
[Result "1-0"]

1. e5 1-0
"#,
        )
        .expect_err("white cannot play e5 from the initial position");

        assert!(matches!(error, ParseGameError::IllegalMove(_)));
    }
}
