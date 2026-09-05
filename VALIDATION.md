# Validation record

2026-09-05: Stages 3–5 passed `cargo test` and strict Clippy. Stockfish 18 at
`/opt/homebrew/bin/stockfish`, SHA-256
`ae4c93fa9676ca7750d0714342fd8a5b1d018000fc6e0f6cedf112067b5ef374`, macOS aarch64,
Threads 1, Hash 64, MultiPV 1, cleared hash per position: two starting-position
searches at 150,000 nodes returned identical +47 cp, e2e4, and a 21-ply PV.
This is observed same-machine stability, not a cross-platform guarantee.
Default embedded network identities: nn-c288c895ea92.nnue and
nn-37f18f62d772.nnue; executable digest pins embedded bytes.

## Completed implementation and final batch

Stages 6–10 and the Stage 11 CLI are implemented. Final automated checks:
`cargo fmt --check`, `cargo test --locked`, and
`cargo clippy --locked --all-targets -- -D warnings`. The default suite contains
69 passing library tests, four CLI/recorded-corpus integration tests, and one
compile-fail doctest (74 checks); two engine-dependent tests are explicitly ignored
in the default run and were run successfully with STOCKFISH set.

The real-engine tests cover same-machine repeated search and the original winning
queen sacrifice at both budgets before any finding cap, plus its full-game scan.
The 12-game acceptance batch completed twice. Both runs used the same pinned engine
and node budgets; the second incorporates the equal-exchange label correction.
Its canonical snapshot is `tests/fixtures/rapid-12-review.json`; offline source
fixtures are `rapid-12.json` and `rapid-12.pgn` in the same directory.

Final batch: **12 games, 22 confirmed findings**. Primary labels:

| Label | Games affected / reviewed | Occurrences |
|---|---:|---:|
| engine-verified-mistake (unclassified mechanism) | 7 / 12 | 14 |
| attacked-piece-ignored | 3 / 12 | 4 |
| line-opened | 2 / 12 | 2 |
| defender-left | 2 / 12 | 2 |

Every persisted move/FEN/SAN/PV passed validation. All recorded findings meet the
200 cp or structured mate-transition threshold, pattern counts equal their source
findings, and no equal rook trade retains a `line-onto` label. Clock annotations
are present in all 12 games; mean time used is 42.5% of base time plus earned
increments. This is descriptive, not evidence that faster play caused losses.

The CLI was exercised against the live API, then the cached batch offline. HTTP
404/429/server errors, immutable month caching, malformed caches, completion
metadata, interrupted UCI iterations, handshake/search cleanup, both player
colours, confirmation-before-capping, claimed repetition, castling both ways,
en passant, promotions and underpromotion have automated coverage.

`review`, `validate`, and JSON-only `render` succeeded for the final batch.
The generated report is in `review-output/acceptance-2026-09-05/report.html` and its
coaching digest is alongside it. A full list for human review is in
`docs/LABEL_REVIEW.md`.

**Release gate remains pending:** human semantic-label review, a useful practice
change, and browser layout inspection. No connected browser was available;
attempting native Safari inspection reported missing computer-use permissions.
HTML structure and escaping passed automated checks, but that is not a screenshot
review. See `docs/ACCEPTANCE.md`. Optional Stages 12–14 have not started.

`cargo build --release --locked` also passed. The optimized CLI successfully
validated and rendered the final review. HTML inspection counted 44 complete
8×8 boards (before and after for each of 22 findings), with no external assets.
