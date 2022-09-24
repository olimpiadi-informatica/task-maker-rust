/// The status of a testcase that got scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoreStatus {
    /// The testcase scored the maximum amount of points.
    Accepted,
    /// The testcase scored more than zero, but not the maximum.
    PartialScore,
    /// The testcase scored zero points.
    WrongAnswer,
}

impl ScoreStatus {
    /// Select the correct status based on the score of a testacase and its maximum possible value.
    pub fn from_score(score: f64, max_score: f64) -> Self {
        if score == 0.0 {
            Self::WrongAnswer
        } else if (score - max_score).abs() < 0.001 {
            Self::Accepted
        } else {
            Self::PartialScore
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ScoreStatus;

    #[test]
    fn test_score_status() {
        assert_eq!(ScoreStatus::from_score(0.0, 1.0), ScoreStatus::WrongAnswer);
        assert_eq!(ScoreStatus::from_score(0.1, 1.0), ScoreStatus::PartialScore);
        assert_eq!(ScoreStatus::from_score(1.0, 1.0), ScoreStatus::Accepted);
        assert_eq!(ScoreStatus::from_score(0.99999, 1.0), ScoreStatus::Accepted);
    }
}
