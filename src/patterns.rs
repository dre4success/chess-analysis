use crate::review::{Breakdown, Classification, Colour, ExampleRef, GameReview, Pattern};
use std::collections::{BTreeMap, BTreeSet};
pub fn phase(ply: u32) -> &'static str {
    if ply <= 20 {
        "opening (plies 1–20)"
    } else if ply <= 60 {
        "middle (plies 21–60)"
    } else {
        "late (plies 61+)"
    }
}
pub fn clock_band(secs: Option<f64>) -> &'static str {
    match secs {
        None => "unknown",
        Some(s) if s < 30.0 => "under 30s",
        Some(s) if s < 120.0 => "30–119s",
        _ => "120s+",
    }
}
fn colour(c: Colour) -> &'static str {
    match c {
        Colour::White => "white",
        Colour::Black => "black",
    }
}
pub fn aggregate(games: &[GameReview]) -> Vec<Pattern> {
    let classes: BTreeSet<_> = games
        .iter()
        .flat_map(|g| g.findings.iter().map(|f| f.classification))
        .collect();
    let mut patterns = Vec::new();
    for classification in classes {
        let mut affected = BTreeSet::new();
        let mut examples = Vec::new();
        let mut groups: BTreeMap<(String, String), (BTreeSet<usize>, u32)> = BTreeMap::new();
        for (i, game) in games.iter().enumerate() {
            for finding in game
                .findings
                .iter()
                .filter(|f| f.classification == classification)
            {
                affected.insert(i);
                examples.push(ExampleRef {
                    url: game.url.clone(),
                    ply: finding.ply,
                });
                for (dimension, value) in [
                    ("colour", colour(game.colour)),
                    ("eco", game.eco.as_deref().unwrap_or("unknown")),
                    ("phase", phase(finding.ply)),
                    ("clock remaining", clock_band(finding.clock_secs)),
                ] {
                    let entry = groups.entry((dimension.into(), value.into())).or_default();
                    entry.0.insert(i);
                    entry.1 += 1;
                }
            }
        }
        let breakdowns = groups
            .into_iter()
            .map(|((dimension, value), (affected, occurrences))| {
                let denominator = games
                    .iter()
                    .filter(|g| match dimension.as_str() {
                        "colour" => colour(g.colour) == value,
                        "eco" => g.eco.as_deref().unwrap_or("unknown") == value,
                        "phase" => g.phase_coverage.contains(&value),
                        _ => g.clock_band_coverage.contains(&value),
                    })
                    .count()
                    .max(affected.len());
                Breakdown {
                    dimension,
                    value,
                    games_affected: affected.len() as u32,
                    games_reviewed: denominator as u32,
                    occurrences,
                }
            })
            .collect();
        examples.sort_by(|a, b| (&a.url, a.ply).cmp(&(&b.url, b.ply)));
        let occurrences = examples.len() as u32;
        examples.truncate(3);
        patterns.push(Pattern {
            classification,
            games_affected: affected.len() as u32,
            games_reviewed: games.len() as u32,
            occurrences,
            example_refs: examples,
            breakdowns,
        });
    }
    patterns.sort_by_key(|p| {
        (
            std::cmp::Reverse(p.games_affected),
            std::cmp::Reverse(p.occurrences),
            p.classification,
        )
    });
    patterns
}
pub fn lesson(c: Classification) -> &'static str {
    match c {
        Classification::LineOpened => {
            "Before moving a blocker, trace enemy bishop, rook and queen lines behind it."
        }
        Classification::LineOnto => {
            "Check legal captures and recaptures on your destination square."
        }
        Classification::DefenderLeft => {
            "Before moving a defender, check which pieces depend on it."
        }
        Classification::AttackedPieceIgnored => {
            "After each opponent move, identify newly attacked pieces before choosing a reply."
        }
        Classification::CaptureCost => "Calculate the complete exchange before capturing.",
        Classification::MissedMate => {
            "Look for forcing checks and mate before choosing a quiet move."
        }
        Classification::EngineVerifiedMistake => {
            "Replay the cited positions and compare your move with the engine alternative."
        }
    }
}
/// Derive elapsed thinking time only when every relevant annotation is present.
/// Increment is credited after each move; negative deltas indicate inconsistent data.
pub fn clock_used_pct(
    game: &crate::completion::CompletedGame,
    user: shakmaty::Color,
) -> Option<f64> {
    use shakmaty::Position;
    let tc = game.headers().get("TimeControl")?;
    let (base, increment) = tc.split_once('+').unwrap_or((tc, "0"));
    let base: f64 = base.parse().ok()?;
    let increment: f64 = increment.parse().ok()?;
    if !base.is_finite() || base <= 0.0 || !increment.is_finite() || increment < 0.0 {
        return None;
    }
    if game.headers().contains_key("FEN") {
        return None;
    }
    let mut previous = base;
    let mut spent = 0.0;
    let mut moves = 0;
    for (i, clock) in game.clocks_ms().iter().enumerate() {
        let side = if i % 2 == 0 {
            game.initial_position().turn()
        } else {
            !game.initial_position().turn()
        };
        if side != user {
            continue;
        }
        let remaining = (*clock)? as f64 / 1000.0;
        let elapsed = previous + increment - remaining;
        if elapsed < -0.01 {
            return None;
        }
        spent += elapsed.max(0.0);
        previous = remaining;
        moves += 1;
    }
    if moves == 0 {
        None
    } else {
        Some(100.0 * spent / (base + increment * f64::from(moves)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clocks_have_units_and_missing_is_not_zero() {
        use crate::completion::CompletionPolicy;
        assert_eq!(crate::pgn::parse_clock_ms("1:02:03.5"), Some(3_723_500));
        assert_eq!(crate::pgn::parse_clock_ms("0:60:00"), None);
        let pgn = b"[Result \"1-0\"]\n[TimeControl \"60+2\"]\n\n1. e4 {[%clk 0:00:57]} e5 {[%clk 0:00:58]} 2. Nf3 {[%clk 0:00:54]} 1-0";
        let game = crate::completion::LocalPgnPolicy
            .verify(crate::pgn::parse_one(pgn).unwrap().unwrap())
            .unwrap();
        assert_eq!(
            clock_used_pct(&game, shakmaty::Color::White),
            Some(100.0 * 10.0 / 64.0)
        );
        assert_eq!(clock_band(None), "unknown");
    }
    #[test]
    fn affected_games_outrank_occurrences_and_references_resolve() {
        let base = crate::review::tests::sample_review().games.remove(0);
        let mut games = vec![];
        for i in 0..6 {
            let mut game = base.clone();
            game.url = format!("game-{i}");
            if i == 0 {
                game.findings = vec![game.findings[0].clone(); 5];
            } else {
                game.findings[0].classification = Classification::MissedMate;
            }
            games.push(game);
        }
        let patterns = aggregate(&games);
        assert_eq!(patterns[0].classification, Classification::MissedMate);
        assert_eq!(patterns[0].games_affected, 5);
        assert_eq!(patterns[0].games_reviewed, 6);
        assert_eq!(patterns, aggregate(&games));
        for p in patterns {
            for e in p.example_refs {
                assert!(
                    games
                        .iter()
                        .any(|g| g.url == e.url && g.findings.iter().any(|f| f.ply == e.ply))
                );
            }
        }
    }
}
