use crate::{
    completion::CompletedGame,
    engine::{Analysis, EngineError, Nodes, PositionEngine, SearchContext},
    evaluation::{Evaluation, ReviewSide},
    review::legal_move,
};
use shakmaty::{Chess, Color, Position};

#[derive(Debug, Clone, Copy)]
pub struct AnalysisConfig {
    pub scan: Nodes,
    pub deep: Nodes,
    pub threshold_cp: u32,
}
impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            scan: Nodes::new(150_000).unwrap(),
            deep: Nodes::new(1_000_000).unwrap(),
            threshold_cp: 200,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Candidate {
    pub(crate) ply: u32,
    pub(crate) before: Chess,
    pub(crate) after: Chess,
    pub(crate) previous: Option<Chess>,
    pub(crate) actual: shakmaty::Move,
    pub(crate) best: Analysis,
    pub(crate) reply: Option<Analysis>,
    pub(crate) eval_after: Evaluation,
    pub(crate) severity: u64,
}
impl Candidate {
    pub fn ply(&self) -> u32 {
        self.ply
    }
    pub fn before(&self) -> &Chess {
        &self.before
    }
    pub fn after(&self) -> &Chess {
        &self.after
    }
    pub fn previous(&self) -> Option<&Chess> {
        self.previous.as_ref()
    }
    pub fn actual(&self) -> shakmaty::Move {
        self.actual
    }
    pub fn best(&self) -> &Analysis {
        &self.best
    }
    pub fn reply(&self) -> Option<&Analysis> {
        self.reply.as_ref()
    }
    pub fn eval_after(&self) -> Evaluation {
        self.eval_after
    }
}
/// CP arithmetic is confined to CP/CP. Mate transitions have their own ranking.
pub fn loss(before: Evaluation, after: Evaluation) -> u64 {
    match (before, after) {
        (Evaluation::Centipawns { value: a, .. }, Evaluation::Centipawns { value: b, .. }) => {
            (i64::from(a) - i64::from(b)).max(0) as u64
        }
        (
            _,
            Evaluation::Mate {
                winner: ReviewSide::User,
                ..
            },
        ) => 0,
        (
            Evaluation::Mate {
                winner: ReviewSide::Opponent,
                ..
            },
            _,
        ) => 0,
        (
            Evaluation::Mate {
                winner: ReviewSide::User,
                ..
            },
            _,
        ) => 1_000_000_000,
        (
            _,
            Evaluation::Mate {
                winner: ReviewSide::Opponent,
                ..
            },
        ) => 2_000_000_000,
    }
}
fn terminal(position: &Chess, user: Color) -> Option<Evaluation> {
    if position.is_checkmate() {
        Some(Evaluation::mate(
            if position.turn() == user {
                ReviewSide::Opponent
            } else {
                ReviewSide::User
            },
            0,
        ))
    } else if position.is_game_over() || position.halfmoves() >= 150 {
        Some(Evaluation::centipawns(0))
    } else {
        None
    }
}
/// Only a completion policy can construct the accepted input type.
/// ```compile_fail
/// use chess_review::{analysis::{candidates, AnalysisConfig}, pgn::RawGame, engine::UciEngine};
/// fn unsafe_input(raw: &RawGame, engine: &mut UciEngine) {
///     candidates(raw, shakmaty::Color::White, engine, AnalysisConfig::default());
/// }
/// ```
pub fn candidates(
    game: &CompletedGame,
    user: Color,
    engine: &mut impl PositionEngine,
    config: AnalysisConfig,
) -> Result<Vec<Candidate>, EngineError> {
    if config.threshold_cp == 0 || config.deep.get() < config.scan.get() {
        return Err(EngineError::Protocol(
            "invalid analysis budgets or zero threshold".into(),
        ));
    }
    let mut position = game.initial_position().clone();
    let key = |p: &Chess| {
        shakmaty::fen::Fen::from_position(p, shakmaty::EnPassantMode::Legal)
            .to_string()
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut history = vec![key(&position)];
    let mut automatic_draw = false;
    let mut previous = None;
    let mut shortlist = Vec::new();
    for (index, uci) in game.moves().iter().enumerate() {
        if automatic_draw || terminal(&position, user).is_some() {
            break;
        }
        let before = position.clone();
        let actual = uci
            .to_move(&position)
            .map_err(|e| EngineError::Protocol(e.to_string()))?;
        position.play_unchecked(actual);
        let after_key = key(&position);
        history.push(after_key.clone());
        let repetitions = history.iter().filter(|k| **k == after_key).count();
        automatic_draw = repetitions >= 5;
        let claimed_draw = index + 1 == game.moves().len()
            && game.outcome() == pgn_reader::KnownOutcome::Draw
            && (repetitions >= 3 || position.halfmoves() >= 100);
        let terminal_after = terminal(&position, user)
            .or_else(|| (automatic_draw || claimed_draw).then_some(Evaluation::centipawns(0)));
        if before.turn() == user {
            let best = engine.analyse_with_history(
                SearchContext {
                    position: &before,
                    initial: game.initial_position(),
                    moves: &game.moves()[..index],
                },
                user,
                config.scan,
            )?;
            let eval_after = match terminal_after {
                Some(e) => e,
                None => {
                    engine
                        .analyse_with_history(
                            SearchContext {
                                position: &position,
                                initial: game.initial_position(),
                                moves: &game.moves()[..=index],
                            },
                            user,
                            config.scan,
                        )?
                        .evaluation
                }
            };
            if loss(best.evaluation, eval_after) >= u64::from((config.threshold_cp / 2).max(1)) {
                shortlist.push((
                    index,
                    before.clone(),
                    position.clone(),
                    previous.clone(),
                    actual,
                    terminal_after,
                ));
            }
        }
        previous = Some(before);
    }
    let mut confirmed = Vec::new();
    for (index, before, after, previous, actual, terminal_after) in shortlist {
        let best = engine.analyse_with_history(
            SearchContext {
                position: &before,
                initial: game.initial_position(),
                moves: &game.moves()[..index],
            },
            user,
            config.deep,
        )?;
        let (eval_after, reply) = match terminal_after {
            Some(e) => (e, None),
            None => {
                let reply = engine.analyse_with_history(
                    SearchContext {
                        position: &after,
                        initial: game.initial_position(),
                        moves: &game.moves()[..=index],
                    },
                    user,
                    config.deep,
                )?;
                (reply.evaluation, Some(reply))
            }
        };
        let severity = loss(best.evaluation, eval_after);
        // A finite search can disagree with itself; never condemn its best move.
        if severity >= u64::from(config.threshold_cp)
            && legal_move(&before, &best.final_best_uci).map_err(EngineError::Protocol)? != actual
            && legal_move(&before, &best.best_uci).map_err(EngineError::Protocol)? != actual
        {
            confirmed.push(Candidate {
                ply: index as u32 + 1,
                before,
                after,
                previous,
                actual,
                best,
                reply,
                eval_after,
                severity,
            });
        }
    }
    confirmed.sort_by_key(|c| (std::cmp::Reverse(c.severity), c.ply));
    confirmed.truncate(3);
    confirmed.sort_by_key(|c| c.ply);
    Ok(confirmed)
}
pub fn findings(
    game: &CompletedGame,
    user: Color,
    engine: &mut impl PositionEngine,
    config: AnalysisConfig,
) -> Result<Vec<crate::review::Finding>, EngineError> {
    Ok(candidates(game, user, engine, config)?
        .iter()
        .map(crate::detectors::classify)
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::{CompletionPolicy, LocalPgnPolicy};
    struct Constant {
        calls: Vec<u64>,
    }
    impl PositionEngine for Constant {
        fn analyse(&mut self, p: &Chess, _: Color, n: Nodes) -> Result<Analysis, EngineError> {
            assert!(!p.is_game_over());
            self.calls.push(n.get());
            let uci = p.legal_moves()[0]
                .to_uci(shakmaty::CastlingMode::Standard)
                .to_string();
            Ok(Analysis {
                evaluation: Evaluation::centipawns(0),
                best_uci: uci.clone(),
                final_best_uci: uci.clone(),
                pv: vec![uci],
            })
        }
    }
    #[test]
    fn delivered_mate_and_even_evaluations_never_flag() {
        let game = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(
                    b"[Result \"1-0\"]\n\n1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0",
                )
                .unwrap()
                .unwrap(),
            )
            .unwrap();
        let mut engine = Constant { calls: vec![] };
        assert!(
            candidates(&game, Color::White, &mut engine, AnalysisConfig::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(engine.calls.len(), 7); // terminal after-position never searched
    }
    #[test]
    fn mate_is_never_subtracted_from_cp() {
        assert_eq!(
            loss(
                Evaluation::centipawns(200),
                Evaluation::mate(ReviewSide::User, 0)
            ),
            0
        );
        assert_eq!(
            loss(
                Evaluation::mate(ReviewSide::Opponent, 2),
                Evaluation::mate(ReviewSide::Opponent, 1)
            ),
            0
        );
        assert!(
            loss(
                Evaluation::mate(ReviewSide::User, 2),
                Evaluation::centipawns(0)
            ) > 200
        );
    }
    #[test]
    #[ignore = "requires STOCKFISH; full promotion-sacrifice regression"]
    fn winning_queen_sacrifice_is_not_flagged() {
        let game = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(include_bytes!("../tests/fixtures/promotion-sacrifice.pgn"))
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let config = AnalysisConfig::default();
        let path = std::env::var("STOCKFISH").unwrap();
        let (mut engine, _) = crate::stockfish::open(
            std::path::Path::new(&path),
            config.scan,
            config.deep,
            std::time::Duration::from_secs(60),
        )
        .unwrap();
        let mut before = game.initial_position().clone();
        for uci in &game.moves()[..53] {
            let mv = uci.to_move(&before).unwrap();
            before.play_unchecked(mv);
        }
        let sacrifice = game.moves()[53].to_move(&before).unwrap();
        assert_eq!(game.moves()[53].to_string(), "a1f1");
        let after = before.clone().play(sacrifice).unwrap();
        for nodes in [config.scan, config.deep] {
            let best = engine.analyse(&before, Color::Black, nodes).unwrap();
            let reply = engine.analyse(&after, Color::Black, nodes).unwrap();
            assert!(loss(best.evaluation, reply.evaluation) < u64::from(config.threshold_cp));
        }
        let findings = candidates(&game, Color::Black, &mut engine, config).unwrap();
        assert!(findings.iter().all(|c| c.ply != 54));
    }
    #[test]
    #[ignore = "requires STOCKFISH; repetition history and causal explanation regressions"]
    fn available_repetition_and_harmless_ray_are_reviewed_correctly() {
        let config = AnalysisConfig::default();
        let path = std::env::var("STOCKFISH").unwrap();
        let (mut engine, _) = crate::stockfish::open(
            std::path::Path::new(&path),
            config.scan,
            config.deep,
            std::time::Duration::from_secs(60),
        )
        .unwrap();
        let repetition = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(include_bytes!("../tests/fixtures/repetition-draw.pgn"))
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let findings = findings(&repetition, Color::Black, &mut engine, config).unwrap();
        let lost_draw = findings
            .iter()
            .find(|f| f.ply == 8)
            .expect("declining the available draw must be found");
        assert_eq!(lost_draw.best_uci.as_deref(), Some("f8g8"));
        assert_eq!(lost_draw.eval_before, Evaluation::centipawns(0));
        assert!(loss(lost_draw.eval_before, lost_draw.eval_after) >= 200);
        assert!(!lost_draw.refutation_variation_uci.is_empty());
        let ray = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(include_bytes!("../tests/fixtures/harmless-ray.pgn"))
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let findings = super::findings(&ray, Color::Black, &mut engine, config).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].classification,
            crate::review::Classification::LineOnto
        );
        assert_eq!(
            findings[0]
                .refutation_variation_uci
                .first()
                .map(String::as_str),
            Some("d3e4")
        );
    }
    #[test]
    fn scan_shortlists_but_only_deep_search_decides() {
        struct Script {
            evals: std::collections::VecDeque<i32>,
            budgets: Vec<u64>,
        }
        impl PositionEngine for Script {
            fn analyse(&mut self, p: &Chess, _: Color, n: Nodes) -> Result<Analysis, EngineError> {
                self.budgets.push(n.get());
                let uci = if p.turn() == Color::White {
                    "d2d4"
                } else {
                    "e7e5"
                }
                .to_owned();
                Ok(Analysis {
                    evaluation: Evaluation::centipawns(self.evals.pop_front().unwrap()),
                    best_uci: uci.clone(),
                    final_best_uci: uci.clone(),
                    pv: vec![uci],
                })
            }
        }
        let game = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(b"[Result \"1-0\"]\n\n1. e4 1-0")
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let mut engine = Script {
            evals: [0, -400, 0, 0].into(),
            budgets: vec![],
        };
        assert!(
            candidates(&game, Color::White, &mut engine, AnalysisConfig::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(engine.budgets, vec![150_000, 150_000, 1_000_000, 1_000_000]);
        let mut engine = Script {
            evals: [0, -400, 0, -300].into(),
            budgets: vec![],
        };
        let confirmed =
            candidates(&game, Color::White, &mut engine, AnalysisConfig::default()).unwrap();
        assert_eq!(confirmed.len(), 1);
        assert_eq!(confirmed[0].eval_after, Evaluation::centipawns(-300));
    }
    #[test]
    fn final_threefold_draw_never_reaches_engine() {
        let game = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(
                    b"[Result \"1/2-1/2\"]\n\n1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8 1/2-1/2",
                )
                .unwrap()
                .unwrap(),
            )
            .unwrap();
        let mut engine = Constant { calls: vec![] };
        assert!(
            candidates(&game, Color::Black, &mut engine, AnalysisConfig::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(engine.calls.len(), 7);
    }

    #[test]
    fn cap_is_applied_after_all_confirmations_for_both_colours() {
        struct E {
            user: Color,
            actuals: Vec<(Chess, shakmaty::Move)>,
            deep_calls: usize,
        }
        impl PositionEngine for E {
            fn analyse(&mut self, p: &Chess, _: Color, n: Nodes) -> Result<Analysis, EngineError> {
                if n.get() == 1_000_000 {
                    self.deep_calls += 1;
                }
                let actual = self.actuals.iter().find(|(b, _)| b == p).map(|(_, m)| *m);
                let mv = p
                    .legal_moves()
                    .into_iter()
                    .find(|m| Some(*m) != actual)
                    .unwrap();
                let u = mv.to_uci(shakmaty::CastlingMode::Standard).to_string();
                Ok(Analysis {
                    evaluation: Evaluation::centipawns(if p.turn() == self.user {
                        0
                    } else {
                        -300
                    }),
                    best_uci: u.clone(),
                    final_best_uci: u.clone(),
                    pv: vec![u],
                })
            }
        }
        let game = LocalPgnPolicy
            .verify(
                crate::pgn::parse_one(
                    b"[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 d6 5. Nc3 Nf6 1-0",
                )
                .unwrap()
                .unwrap(),
            )
            .unwrap();
        let mut p = game.initial_position().clone();
        let mut actuals = vec![];
        for u in game.moves() {
            let m = u.to_move(&p).unwrap();
            actuals.push((p.clone(), m));
            p.play_unchecked(m);
        }
        for user in [Color::White, Color::Black] {
            let mut e = E {
                user,
                actuals: actuals.clone(),
                deep_calls: 0,
            };
            let c = candidates(&game, user, &mut e, AnalysisConfig::default()).unwrap();
            assert_eq!(c.len(), 3);
            assert_eq!(e.deep_calls, 10);
        }
    }
}
