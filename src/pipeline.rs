use crate::{
    analysis::{self, AnalysisConfig},
    completion::CompletedGame,
    patterns,
    review::{Colour, GameResult, GameReview, Opponent},
};
use shakmaty::{Color, Position};
use std::path::{Path, PathBuf};
pub struct Input {
    pub game: CompletedGame,
    pub url: String,
    pub hash: String,
}
pub fn executable(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if path.components().count() > 1 {
        return Ok(path.canonicalize()?);
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return Ok(candidate.canonicalize()?);
        }
    }
    Err(format!("engine executable not found: {}", path.display()).into())
}
pub fn game_review(
    input: &Input,
    user: &str,
    engine: &mut impl crate::engine::PositionEngine,
    config: AnalysisConfig,
) -> Result<GameReview, Box<dyn std::error::Error>> {
    let game = &input.game;
    let h = game.headers();
    let colour = if h.get("White").is_some_and(|n| n.eq_ignore_ascii_case(user)) {
        Color::White
    } else if h.get("Black").is_some_and(|n| n.eq_ignore_ascii_case(user)) {
        Color::Black
    } else {
        return Err("reviewed user is not a player".into());
    };
    let mut findings = analysis::findings(game, colour, engine, config)?;
    for f in &mut findings {
        f.clock_secs = game
            .clocks_ms()
            .get(f.ply as usize - 1)
            .copied()
            .flatten()
            .map(|ms| ms as f64 / 1000.0);
    }
    let opponent_tag = if colour == Color::White {
        "Black"
    } else {
        "White"
    };
    let mut phase_coverage = std::collections::BTreeSet::new();
    let mut clock_band_coverage = std::collections::BTreeSet::new();
    for (i, clock) in game.clocks_ms().iter().enumerate() {
        let side = if i % 2 == 0 {
            game.initial_position().turn()
        } else {
            !game.initial_position().turn()
        };
        if side == colour {
            phase_coverage.insert(patterns::phase(i as u32 + 1).to_owned());
            clock_band_coverage
                .insert(patterns::clock_band(clock.map(|v| v as f64 / 1000.0)).to_owned());
        }
    }
    Ok(GameReview {
        url: input.url.clone(),
        input_pgn_sha256: input.hash.clone(),
        date: h
            .get("UTCDate")
            .or_else(|| h.get("Date"))
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
        colour: if colour == Color::White {
            Colour::White
        } else {
            Colour::Black
        },
        result: match game.outcome() {
            pgn_reader::KnownOutcome::Draw => GameResult::Draw,
            pgn_reader::KnownOutcome::Decisive { winner } if winner == colour => GameResult::Win,
            _ => GameResult::Loss,
        },
        opponent: Opponent {
            name: h
                .get(opponent_tag)
                .cloned()
                .unwrap_or_else(|| "unknown".into()),
            rating: h
                .get(&format!("{opponent_tag}Elo"))
                .and_then(|v| v.parse().ok()),
        },
        time_control: h
            .get("TimeControl")
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
        clock_used_pct: patterns::clock_used_pct(game, colour),
        eco: h.get("ECO").cloned(),
        findings,
        phase_coverage: phase_coverage.into_iter().collect(),
        clock_band_coverage: clock_band_coverage.into_iter().collect(),
    })
}
