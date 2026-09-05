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
            {
                return Some(evidence(
                    Classification::LineOpened,
                    format!(
                        "Moving from {from} opened the {attacker}–{victim} line onto your {} on {victim}.",
                        ctx.after.board().role_at(victim)?.char()
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
}
