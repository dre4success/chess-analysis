//! Observations only: these functions cannot produce an engine verdict or Finding.
use crate::review::Classification;
use shakmaty::{Chess, Move, Position, Role, attacks};

pub struct MoveContext<'a> {
    pub before: &'a Chess,
    pub after: &'a Chess,
    pub actual: Move,
}
#[derive(Debug, Clone)]
pub struct ClassificationEvidence {
    pub classification: Classification,
    pub explanation: String,
}
fn evidence(classification: Classification, explanation: String) -> ClassificationEvidence {
    ClassificationEvidence {
        classification,
        explanation,
    }
}
pub fn line_opened(ctx: &MoveContext<'_>) -> Option<ClassificationEvidence> {
    let from = ctx.actual.from()?;
    let user = ctx.before.turn();
    for victim in ctx.after.board().by_color(user) {
        if victim == ctx.actual.to()
            || ctx.before.board().piece_at(victim) != ctx.after.board().piece_at(victim)
        {
            continue;
        }
        let attackers = ctx
            .after
            .board()
            .attacks_to(victim, !user, ctx.after.board().occupied());
        for attacker in attackers {
            if matches!(
                ctx.after.board().role_at(attacker),
                Some(Role::Bishop | Role::Rook | Role::Queen)
            ) && attacks::between(attacker, victim).contains(from)
                && !ctx.before.board().attacks_from(attacker).contains(victim)
                && exchange_gain(ctx.after, victim) > 0
            {
                return Some(evidence(
                    Classification::LineOpened,
                    format!(
                        "Moving from {from} opened the {attacker}–{victim} line onto your {} on {victim}.",
                        piece_name(ctx.after.board().role_at(victim)?)
                    ),
                ));
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Square;
    pub fn position(fen: &str) -> Chess {
        fen.parse::<shakmaty::fen::Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap()
    }
    fn probe(fen: &str, uci: &str) -> Option<ClassificationEvidence> {
        let before = position(fen);
        let actual = crate::review::legal_move(&before, uci).unwrap();
        let after = before.clone().play(actual).unwrap();
        line_opened(&MoveContext {
            before: &before,
            after: &after,
            actual,
        })
    }
    #[test]
    fn blocker_leaves_bishop_ray() {
        let fen = "3qk3/8/5n2/8/7B/8/8/4K3 b - - 0 1";
        let before = position(fen);
        assert!(attacks::between(Square::H4, Square::D8).contains(Square::F6));
        assert!(!before.board().attacks_from(Square::H4).contains(Square::D8));
        assert_eq!(
            probe(fen, "f6e4").unwrap().classification,
            Classification::LineOpened
        );
    }
    #[test]
    fn queen_leaving_defence_is_not_opening_a_ray() {
        let fen = "3qk3/8/5n2/8/7B/8/8/4K3 b - - 0 1";
        assert!(
            position(fen)
                .board()
                .attacks_from(Square::H4)
                .contains(Square::F6)
        );
        assert!(probe(fen, "d8c7").is_none());
    }
    #[test]
    fn existing_line_and_no_victim_are_negative() {
        let fen = "3qk3/8/8/8/7B/8/1n6/4K3 b - - 0 1";
        assert!(
            position(fen)
                .board()
                .attacks_from(Square::H4)
                .contains(Square::D8)
        );
        assert!(probe(fen, "b2a4").is_none());
        assert!(probe("4k3/8/5n2/8/7B/8/8/4K3 b - - 0 1", "f6e4").is_none());
    }
    #[test]
    fn harmless_ray_does_not_override_hanging_the_moved_knight() {
        use crate::{analysis::Candidate, engine::Analysis, evaluation::Evaluation};
        let before = position("5k2/4p1p1/5n2/8/7B/3P4/8/4K3 b - - 0 1");
        let actual = crate::review::legal_move(&before, "f6e4").unwrap();
        let after = before.clone().play(actual).unwrap();
        assert_eq!(exchange_gain(&after, Square::E7), 0);
        assert_eq!(exchange_gain(&after, Square::E4), 320);
        let finding = classify(&Candidate {
            ply: 1,
            before,
            after,
            previous: None,
            actual,
            best: Analysis {
                evaluation: Evaluation::centipawns(47),
                best_uci: "f8f7".into(),
                final_best_uci: "f8f7".into(),
                pv: vec!["f8f7".into()],
            },
            reply: Some(Analysis {
                evaluation: Evaluation::centipawns(-515),
                best_uci: "d3e4".into(),
                final_best_uci: "d3e4".into(),
                pv: vec!["d3e4".into()],
            }),
            eval_after: Evaluation::centipawns(-515),
            severity: 562,
        });
        assert_eq!(finding.classification, Classification::LineOnto);
        assert!(!finding.also_matched.contains(&Classification::LineOpened));
        assert_eq!(finding.refutation_variation_uci, vec!["d3e4"]);
    }
}

fn piece_name(role: Role) -> &'static str {
    match role {
        Role::Pawn => "pawn",
        Role::Knight => "knight",
        Role::Bishop => "bishop",
        Role::Rook => "rook",
        Role::Queen => "queen",
        Role::King => "king",
    }
}
fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight => 320,
        Role::Bishop => 330,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 0,
    }
}
/// Maximum legal capture gain on one square. Regenerating legal moves handles
/// pins, x-rays, en passant and capture-promotions. This is not a position eval.
pub fn exchange_gain(position: &Chess, square: shakmaty::Square) -> i32 {
    position
        .legal_moves()
        .iter()
        .filter(|m| m.to() == square && m.is_capture())
        .map(|mv| {
            let gain = mv.capture().map(value).unwrap_or(100)
                + mv.promotion().map(|p| value(p) - 100).unwrap_or(0);
            let after = position.clone().play(*mv).expect("generated legal capture");
            gain - exchange_gain(&after, square)
        })
        .max()
        .unwrap_or(0)
        .max(0)
}
pub fn line_onto(ctx: &MoveContext<'_>) -> Option<ClassificationEvidence> {
    let square = ctx.actual.to();
    let material_gained = ctx.actual.capture().map(value).unwrap_or(0)
        + ctx.actual.promotion().map(|p| value(p) - 100).unwrap_or(0);
    if exchange_gain(ctx.after, square) > material_gained {
        Some(evidence(
            Classification::LineOnto,
            format!(
                "Your {} landed on {square}, where the opponent has a profitable legal capture sequence.",
                piece_name(ctx.actual.role())
            ),
        ))
    } else {
        None
    }
}
pub fn capture_cost(ctx: &MoveContext<'_>) -> Option<ClassificationEvidence> {
    let captured = ctx.actual.capture()?;
    let gained = value(captured) + ctx.actual.promotion().map(|p| value(p) - 100).unwrap_or(0);
    if exchange_gain(ctx.after, ctx.actual.to()) > gained {
        Some(evidence(
            Classification::CaptureCost,
            format!(
                "The capture on {} gains less material than the opponent can win back on that square.",
                ctx.actual.to()
            ),
        ))
    } else {
        None
    }
}
pub fn defender_left(ctx: &MoveContext<'_>) -> Option<ClassificationEvidence> {
    let from = ctx.actual.from()?;
    let user = ctx.before.turn();
    for victim in ctx.after.board().by_color(user) {
        if victim == ctx.actual.to()
            || ctx.before.board().piece_at(victim) != ctx.after.board().piece_at(victim)
        {
            continue;
        }
        let before = ctx
            .before
            .board()
            .attacks_to(victim, user, ctx.before.board().occupied());
        let after = ctx
            .after
            .board()
            .attacks_to(victim, user, ctx.after.board().occupied());
        if before.count() == 1
            && before.contains(from)
            && after.is_empty()
            && exchange_gain(ctx.after, victim) > 0
        {
            return Some(evidence(
                Classification::DefenderLeft,
                format!(
                    "Moving from {from} removed the only geometric defender of your piece on {victim}; the opponent can now win material there."
                ),
            ));
        }
    }
    None
}
pub fn attacked_piece_ignored(
    ctx: &MoveContext<'_>,
    previous: Option<&Chess>,
) -> Option<ClassificationEvidence> {
    let previous = previous?;
    let user = ctx.before.turn();
    for victim in ctx.after.board().by_color(user) {
        if victim == ctx.actual.to()
            || ctx.before.board().piece_at(victim) != ctx.after.board().piece_at(victim)
        {
            continue;
        }
        let old = previous
            .board()
            .attacks_to(victim, !user, previous.board().occupied());
        let new = ctx
            .before
            .board()
            .attacks_to(victim, !user, ctx.before.board().occupied());
        let old_defenders =
            ctx.before
                .board()
                .attacks_to(victim, user, ctx.before.board().occupied());
        let new_defenders =
            ctx.after
                .board()
                .attacks_to(victim, user, ctx.after.board().occupied());
        if old.is_empty()
            && !new.is_empty()
            && (new_defenders & !old_defenders).is_empty()
            && !ctx.actual.is_capture()
            && !ctx.after.is_check()
            && exchange_gain(ctx.after, victim) > 0
        {
            return Some(evidence(
                Classification::AttackedPieceIgnored,
                format!(
                    "The opponent's last move attacked your piece on {victim}; your reply left a profitable capture available there."
                ),
            ));
        }
    }
    None
}
pub fn missed_mate(
    before: crate::evaluation::Evaluation,
    after: crate::evaluation::Evaluation,
) -> Option<ClassificationEvidence> {
    use crate::evaluation::{Evaluation, ReviewSide};
    if matches!(
        before,
        Evaluation::Mate {
            winner: ReviewSide::User,
            moves: 1..=2,
            ..
        }
    ) && !matches!(
        after,
        Evaluation::Mate {
            winner: ReviewSide::User,
            ..
        }
    ) {
        Some(evidence(
            Classification::MissedMate,
            "A forced mate in one or two was available; this move gives up that forced mate."
                .into(),
        ))
    } else {
        None
    }
}
/// Explicit precedence. The pipeline alone converts confirmed candidates to findings.
pub(crate) fn classify(candidate: &crate::analysis::Candidate) -> crate::review::Finding {
    use shakmaty::{CastlingMode, EnPassantMode, fen::Fen, san::SanPlus};
    let ctx = MoveContext {
        before: &candidate.before,
        after: &candidate.after,
        actual: candidate.actual,
    };
    let mut matches: Vec<_> = [
        missed_mate(candidate.best.evaluation, candidate.eval_after),
        line_opened(&ctx),
        defender_left(&ctx),
        attacked_piece_ignored(&ctx, candidate.previous.as_ref()),
        capture_cost(&ctx),
        line_onto(&ctx),
    ]
    .into_iter()
    .flatten()
    .collect();
    if matches.is_empty() {
        matches.push(evidence(Classification::EngineVerifiedMistake, "The deeper engine search confirms an evaluation loss; the tactical mechanism is unclassified.".into()));
    }
    let primary = matches.remove(0);
    let best = crate::review::legal_move(&candidate.before, &candidate.best.best_uci)
        .expect("engine validates best move");
    crate::review::Finding {
        ply: candidate.ply,
        before_fen: Fen::from_position(&candidate.before, EnPassantMode::Legal).to_string(),
        after_fen: Fen::from_position(&candidate.after, EnPassantMode::Legal).to_string(),
        actual_uci: candidate.actual.to_uci(CastlingMode::Standard).to_string(),
        actual_san: SanPlus::from_move(candidate.before.clone(), candidate.actual).to_string(),
        best_uci: Some(candidate.best.best_uci.clone()),
        best_san: Some(SanPlus::from_move(candidate.before.clone(), best).to_string()),
        eval_before: candidate.best.evaluation,
        eval_after: candidate.eval_after,
        classification: primary.classification,
        explanation: primary.explanation,
        also_matched: matches.into_iter().map(|m| m.classification).collect(),
        principal_variation_uci: candidate.best.pv.clone(),
        refutation_variation_uci: candidate
            .reply
            .as_ref()
            .map(|r| r.pv.clone())
            .unwrap_or_default(),
        confidence: crate::review::Confidence::Verified,
        clock_secs: None,
    }
}
#[cfg(test)]
mod exchange_tests {
    use super::*;
    use shakmaty::{CastlingMode, Square, fen::Fen};
    fn pos(f: &str) -> Chess {
        f.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }
    fn check(
        f: &str,
        u: &str,
        detector: fn(&MoveContext<'_>) -> Option<ClassificationEvidence>,
    ) -> bool {
        let before = pos(f);
        let actual = crate::review::legal_move(&before, u).unwrap();
        let after = before.clone().play(actual).unwrap();
        detector(&MoveContext {
            before: &before,
            after: &after,
            actual,
        })
        .is_some()
    }
    #[test]
    fn landing_and_capture_cost_with_near_misses() {
        let f = "7k/8/8/2p5/3p4/8/3Q4/7K w - - 0 1";
        assert!(pos(f).board().attacks_from(Square::C5).contains(Square::D4));
        assert!(check(f, "d2d4", line_onto));
        assert!(check(f, "d2d4", capture_cost));
        assert!(!check(f, "d2e2", line_onto));
        assert!(!check(f, "d2e2", capture_cost));
        let safe = "7k/8/8/8/3p4/8/3Q4/7K w - - 0 1";
        assert!(pos(safe).board().by_color(shakmaty::Color::Black).count() == 2);
        assert!(!check(safe, "d2d4", line_onto));
        assert!(!check(safe, "d2d4", capture_cost));
    }
    #[test]
    fn sole_defender_leaves_and_two_near_misses() {
        let f = "3qk3/8/5n2/8/7B/8/8/4K3 b - - 0 1";
        assert_eq!(
            pos(f)
                .board()
                .attacks_to(
                    Square::F6,
                    shakmaty::Color::Black,
                    pos(f).board().occupied()
                )
                .count(),
            1
        );
        assert!(check(f, "d8c7", defender_left));
        assert!(!check(f, "d8e7", defender_left)); // queen still guards f6
        assert!(!check(
            "3qk3/8/5n2/8/8/8/8/4K3 b - - 0 1",
            "d8c7",
            defender_left
        ));
    }
    #[test]
    fn missed_mate_positive_and_near_misses() {
        use crate::evaluation::{Evaluation as E, ReviewSide as S};
        assert!(missed_mate(E::mate(S::User, 2), E::centipawns(50)).is_some());
        assert!(missed_mate(E::mate(S::User, 2), E::mate(S::User, 3)).is_none());
        assert!(missed_mate(E::mate(S::Opponent, 2), E::centipawns(50)).is_none());
    }
    #[test]
    fn newly_attacked_piece_ignored_or_saved() {
        let previous = pos("7k/8/8/8/8/8/5R2/2b4K b - - 0 1");
        let before = previous
            .clone()
            .play(crate::review::legal_move(&previous, "c1e3").unwrap())
            .unwrap();
        assert!(before.board().attacks_from(Square::E3).contains(Square::F2));
        assert!(
            previous
                .board()
                .attacks_to(
                    Square::F2,
                    shakmaty::Color::Black,
                    previous.board().occupied()
                )
                .is_empty()
        );
        for (uci, expected) in [("h1h2", true), ("f2f3", false)] {
            let actual = crate::review::legal_move(&before, uci).unwrap();
            let after = before.clone().play(actual).unwrap();
            let ctx = MoveContext {
                before: &before,
                after: &after,
                actual,
            };
            assert_eq!(
                attacked_piece_ignored(&ctx, Some(&previous)).is_some(),
                expected
            );
            assert!(attacked_piece_ignored(&ctx, None).is_none());
        }
    }
    #[test]
    fn see_handles_capture_promotions_and_pinned_recaptures() {
        let p = pos("1r5k/P7/8/8/8/8/8/7K w - - 0 1");
        assert!(
            p.legal_moves()
                .iter()
                .any(|m| m.promotion().is_some() && m.is_capture())
        );
        assert_eq!(exchange_gain(&p, Square::B8), 1300);
        let p = pos("4k3/4n3/8/3Q4/8/8/8/4R1K1 b - - 0 1");
        assert!(p.board().attacks_from(Square::E7).contains(Square::D5));
        assert_eq!(exchange_gain(&p, Square::D5), 0);
    }
    #[test]
    fn ordinary_equal_rook_trade_has_no_landing_or_capture_cost_label() {
        let fen = "8/pp2kppp/8/3p1p2/3RrP2/P1P4P/1P4P1/5K2 b - - 0 26";
        let before = pos(fen);
        let mv = crate::review::legal_move(&before, "e4d4").unwrap();
        assert_eq!(mv.capture(), Some(Role::Rook));
        let after = before.clone().play(mv).unwrap();
        assert_eq!(exchange_gain(&after, Square::D4), 500);
        assert!(!check(fen, "e4d4", line_onto));
        assert!(!check(fen, "e4d4", capture_cost));
    }
}
