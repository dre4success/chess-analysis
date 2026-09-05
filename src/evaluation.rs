use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewSide {
    User,
    Opponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvaluationPov {
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Evaluation {
    #[serde(rename = "cp")]
    Centipawns { value: i32, pov: EvaluationPov },
    Mate {
        winner: ReviewSide,
        moves: u16,
        pov: EvaluationPov,
    },
}

impl Evaluation {
    /// Produces a total ordering across centipawn and mate evaluations.
    ///
    /// This value is for ranking only. It must not be serialized or used
    /// as a centipawn value.
    #[must_use]
    pub fn ordering_score(&self) -> i64 {
        match self {
            Self::Centipawns { value, .. } => i64::from(*value),
            Self::Mate {
                winner: ReviewSide::User,
                moves,
                ..
            } => i64::MAX - i64::from(*moves),
            Self::Mate {
                winner: ReviewSide::Opponent,
                moves,
                ..
            } => i64::MIN + i64::from(*moves),
        }
    }

    #[must_use]
    pub const fn centipawns(value: i32) -> Self {
        Self::Centipawns {
            value,
            pov: EvaluationPov::User,
        }
    }

    #[must_use]
    pub const fn mate(winner: ReviewSide, moves: u16) -> Self {
        Self::Mate {
            winner,
            moves,
            pov: EvaluationPov::User,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Evaluation, ReviewSide};
    use serde_json::json;

    fn cp(value: i32) -> Evaluation {
        Evaluation::Centipawns {
            value,
            pov: super::EvaluationPov::User,
        }
    }

    fn mate(winner: ReviewSide, moves: u16) -> Evaluation {
        Evaluation::mate(winner, moves)
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

    #[test]
    fn serializes_centipawns_with_explicit_user_pov() {
        let value = serde_json::to_value(cp(230)).expect("evaluation should serialize");

        assert_eq!(
            value,
            json!({
                "type": "cp",
                "value": 230,
                "pov": "user"
            })
        );
    }

    #[test]
    fn serializes_mate_with_explicit_winner_and_user_pov() {
        let value = serde_json::to_value(mate(ReviewSide::User, 3))
            .expect("mate evaluation should serialize");

        assert_eq!(
            value,
            json!({
                "type": "mate",
                "winner": "user",
                "moves": 3,
                "pov": "user"
            })
        );
    }

    #[test]
    fn both_evaluation_variants_round_trip_through_json() {
        let evaluations = [
            cp(-610),
            mate(ReviewSide::User, 3),
            mate(ReviewSide::Opponent, 7),
        ];

        for evaluation in evaluations {
            let json = serde_json::to_string(&evaluation).expect("evaluation should serialize");
            let decoded: Evaluation =
                serde_json::from_str(&json).expect("evaluation should deserialize");

            assert_eq!(decoded, evaluation);
        }
    }

    #[test]
    fn rejects_an_evaluation_with_opponent_pov() {
        let result = serde_json::from_value::<Evaluation>(json!({
            "type": "cp",
            "value": 20,
            "pov": "opponent"
        }));

        assert!(result.is_err());
    }
}
