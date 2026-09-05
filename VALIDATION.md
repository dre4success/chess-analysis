# Validation record

2026-09-05: Stages 3–5 passed `cargo test` and strict Clippy. Stockfish 18 at
`/opt/homebrew/bin/stockfish`, SHA-256
`ae4c93fa9676ca7750d0714342fd8a5b1d018000fc6e0f6cedf112067b5ef374`, macOS aarch64,
Threads 1, Hash 64, MultiPV 1, cleared hash per position: two starting-position
searches at 150,000 nodes returned identical +47 cp, e2e4, and a 21-ply PV.
This is observed same-machine stability, not a cross-platform guarantee.
Default embedded network identities: nn-c288c895ea92.nnue and
nn-37f18f62d772.nnue; executable digest pins embedded bytes.
