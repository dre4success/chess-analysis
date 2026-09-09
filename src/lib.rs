pub mod completion;
pub mod evaluation;
pub mod pgn;
pub mod review;

pub fn project_name() -> &'static str {
    "chess-review"
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn reports_project_name() {
        assert_eq!(project_name(), "chess-review")
    }
}

pub mod analysis;
pub mod detectors;
pub mod engine;
pub mod fetch;
pub mod patterns;
pub mod render;
pub mod stockfish;

pub mod browser;

#[cfg(not(target_arch = "wasm32"))]
pub mod pipeline;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
