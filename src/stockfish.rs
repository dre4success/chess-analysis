use crate::{
    engine::{EngineError, Nodes, UciEngine},
    review::EngineMetadata,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}
pub fn bytes_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn open(
    path: &Path,
    scan: Nodes,
    deep: Nodes,
    timeout: Duration,
) -> Result<(UciEngine, EngineMetadata), EngineError> {
    if deep.get() < scan.get() {
        return Err(EngineError::Protocol(
            "deep nodes must be at least scan nodes".into(),
        ));
    }
    let executable_sha256 = file_sha256(path)?;
    let mut engine = UciEngine::spawn(path, timeout)?;
    engine.configure()?;
    let mut uci_options = BTreeMap::new();
    for (name, description) in &engine.options {
        let default = description
            .split_once(" default ")
            .map(|(_, s)| {
                s.split(" min ")
                    .next()
                    .unwrap_or(s)
                    .split(" var ")
                    .next()
                    .unwrap_or(s)
            })
            .unwrap_or("");
        uci_options.insert(name.clone(), default.to_owned());
    }
    for (name, value) in [("Threads", "1"), ("Hash", "64"), ("MultiPV", "1")] {
        uci_options.insert(name.into(), value.into());
    }
    let nnue_identity = uci_options
        .iter()
        .filter(|(k, _)| k.starts_with("EvalFile"))
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    // Stockfish embeds the default nets in the executable. Its digest pins their
    // bytes; filenames additionally record the engine-reported network identity.
    let metadata = EngineMetadata {
        name: "Stockfish".into(),
        version: engine.name.clone(),
        executable_sha256,
        nnue_sha256: None,
        nnue_identity,
        uci_options,
        scan_nodes: scan.get(),
        deep_nodes: deep.get(),
        threads: 1,
        hash_mb: 64,
        multipv: 1,
        clear_hash_between_positions: true,
        analysis_order:
            "game-order; scan before/after; confirm before/after; clear hash each position".into(),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    };
    Ok((engine, metadata))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::PositionEngine;
    #[test]
    #[ignore = "requires STOCKFISH executable; records same-machine stability"]
    fn real_engine_repeats_and_returns_legal_moves() {
        let path =
            std::env::var("STOCKFISH").expect("set STOCKFISH to an absolute executable path");
        let nodes = Nodes::new(150_000).unwrap();
        let (mut engine, metadata) =
            open(Path::new(&path), nodes, nodes, Duration::from_secs(30)).unwrap();
        let first = engine
            .analyse(&shakmaty::Chess::default(), shakmaty::Color::White, nodes)
            .unwrap();
        let second = engine
            .analyse(&shakmaty::Chess::default(), shakmaty::Color::White, nodes)
            .unwrap();
        eprintln!("Engine: {metadata:?}\nFirst: {first:?}\nSecond: {second:?}");
        assert_eq!(first, second);
    }
}
