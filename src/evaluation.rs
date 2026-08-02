#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSide {
    User,
    Opponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evaluation {
    Centipawns { value: i32 },
    Mate { winner: ReviewSide, moves: u16 },
}

impl Evaluation {
    /// Produces a total ordering across centipawn and mate evaluations.
    ///
    /// This value is for ranking only. It must not be serialized or used
    /// as a centipawn value.
    #[must_use]
    pub fn ordering_score(&self) -> i64 {
        match self {
            Self::Centipawns { value } => i64::from(*value),
            Self::Mate {
                winner: ReviewSide::User,
                moves,
            } => i64::MAX - i64::from(*moves),
            Self::Mate {
                winner: ReviewSide::Opponent,
                moves,
            } => i64::MIN + i64::from(*moves),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Evaluation, ReviewSide};

    fn cp(value: i32) -> Evaluation {
        Evaluation::Centipawns { value }
    }

    fn mate(winner: ReviewSide, moves: u16) -> Evaluation {
        Evaluation::Mate { winner, moves }
    }

    #[test]
    fn centipawn_order_is_preserved() {
        assert!(cp(-50).ordering_score() < cp(20).ordering_score());
    }

    #[test]
    fn winning_mate_outranks_every_centipawn_evaluation() {
        assert!(mate(ReviewSide::User, u16::MAX).ordering_score() > cp(i32::MAX).ordering_score())
    }

    #[test]
    fn losing_mate_is_worse_than_every_centipawn_evaluation() {
        assert!(
            mate(ReviewSide::Opponent, u16::MAX).ordering_score() < cp(i32::MIN).ordering_score()
        )
    }

    #[test]
    fn slower_losing_mate_is_better() {
        assert!(
            mate(ReviewSide::Opponent, 5).ordering_score()
                > mate(ReviewSide::Opponent, 2).ordering_score()
        )
    }

    #[test]
    fn faster_winning_mate_is_better() {
        assert!(
            mate(ReviewSide::User, 2).ordering_score() > mate(ReviewSide::User, 5).ordering_score()
        )
    }
}
