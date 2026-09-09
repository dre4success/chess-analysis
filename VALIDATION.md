# Validation record

## Traefik production preparation — 2026-09-09

- Added standalone `compose.prod.yaml` for the existing `web_proxy_net`, `web`/`websecure` entrypoints and `myresolver`, following Sela's production Compose file. Tempo has explicit service port 8080, a namespaced HTTPS redirect, optional existing middleware, no published host port and the same persistent volume/runtime limits as local Compose.
- Adapted GitHub Actions to install the production manifest as `docker-compose.yml` under the configured app directory, preserve the manually managed `.env`, and persist the successful image digest in `tag.env`. The deployment checks for `.env` and the proxy network before pulling or starting the app.
- `scripts/check-compose.py` passed and now runs in CI. Actual Compose interpolation also passed with separate fixture `.env` and `tag.env` files. All three workflows passed actionlint; shell syntax and Linux Bash deployment-argument checks passed.
- Simulated the remote deployment with a stub Docker command: success, unhealthy startup, missing proxy network and missing `.env` all behaved as intended. Existing `.env` content was unchanged; failed attempts preserved the previous `tag.env` and cleaned up temporary files.
- These are local configuration and workflow checks. No server connection, image publication, DNS change or production deployment was performed; the actual VPS, TLS issuance and hostname remain unverified.

## Username and stale chart fixes — 2026-09-09

- The username field now uses a shared HTML pattern with an escaped hyphen. A regression test compiles the actual pattern with the browser's Unicode-set `v` flag and checks valid and invalid usernames.
- Rust serves HTML, API responses and missing files with `Cache-Control: no-store`; successful Vite assets are immutable. Failed chart imports trigger one guarded reload of the current URL. Offline, repeated and other module failures reach a local panel boundary instead of removing the dashboard.
- All **15 frontend tests**, TypeScript, Oxlint, Prettier and the production build passed. Rust passed **79 unit tests, 4 CLI integration tests and 1 doctest**, plus formatting and strict Clippy. Three existing real-engine tests remain opt-in and were not rerun for this change. The HTTP test required local loopback access outside the sandbox.
- Rebuilt the local Linux ARM64 Docker image. Container smoke checks passed, including the actual lazy chart dependency, cache headers, missing-chunk 404s and restart persistence. Refreshed Compose on port 8080; its entry and chart bytes match the local build. All **3 completed reviews** have identical response SHA-256 hashes before and after the update. No remote deployment was performed.

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

| Label                                            | Games affected / reviewed | Occurrences |
| ------------------------------------------------ | ------------------------: | ----------: |
| engine-verified-mistake (unclassified mechanism) |                    7 / 12 |          14 |
| attacked-piece-ignored                           |                    3 / 12 |           4 |
| line-opened                                      |                    2 / 12 |           2 |
| defender-left                                    |                    2 / 12 |           2 |

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
coaching digest is alongside it. Human review notes are maintained locally.

**Release gate remains pending:** human semantic-label review, a useful practice
change, and browser layout inspection. No connected browser was available;
attempting native Safari inspection reported missing computer-use permissions.
HTML structure and escaping passed automated checks, but that is not a screenshot
review. Optional Stages 12–14 have not started.

`cargo build --release --locked` also passed. The optimized CLI successfully
validated and rendered the final review. HTML inspection counted 44 complete
8×8 boards (before and after for each of 22 findings), with no external assets.

## Tempo rebuild — 2026-09-08

Private app: https://chess-focus-dre.dre4success.chatgpt.site

- Live username import returned 40 rated rapid games for `dre4success007` from the production Worker build; invalid usernames returned HTTP 400. The homepage returned HTTP 200.
- The original Rust completion guard, UCI parser, candidate selection, detectors and default budgets compile into `web/public/rust/chess_review.wasm`. Only transport and asynchronous search scheduling are adapted. CLI I/O stays native.
- Native checks: 72 unit tests, 4 CLI integration tests and 1 compile-fail doctest passed; 2 pre-existing real-engine tests remain opt-in. Formatting and Clippy passed.
- Web checks: all 5 contract tests, TypeScript, Prettier and the deployment build passed.
- Real engine parity: native Rust CLI and compiled Rust, using the identical Stockfish 18 Lite WASM binary, produced the same 2 findings for the 27-ply game against `zakmar123`, including evaluations, moves, labels and PVs. This does not claim cross-network equivalence with full native Stockfish.
- The private deployment completed successfully. Browser handoff was unavailable because no browser was connected, so visual/interaction QA is still unverified. Optional WebMCP registration also lacks a supported validation context.
- The scaffold's patchable security advisories were updated. Four remaining npm audit entries all track the development dependency chain through Sharp/libheif in Wrangler/Miniflare; the latest upstream versions still report the advisory. The app does not expose image uploads or image decoding. No forced downgrade was applied.

Formatter commands: `npm --prefix web run format` and `npm --prefix web run format:check` use Prettier. Rust continues to use `cargo fmt`.

## Native Tempo server and studio redesign — 2026-09-09

- Replaced the primary frontend build with static Vite output served by Rust/Axum. The production build contains no Stockfish Lite worker or Rust WASM download. The previous browser adapter is retained as an optional prototype and regression target.
- Rebuilt the welcome, overview and review styling in graphite, ivory and lime, with a real interactive board on entry, a featured position above the statistics, locally bundled fonts, responsive layouts and a saved-review shelf. Rating charts load on demand. Initial JS is approximately 101 kB gzipped; CSS approximately 11 kB gzipped.
- Normal Rust suite: **78 unit tests, 4 CLI integration tests, 1 compile-fail doctest passed**. Three real-engine tests are opt-in; the new native server test was run separately and passed. It analyses a real game, validates its review, confirms zero-search cache reuse and invalidates the cache when the core changes. The two earlier opt-in tests were not rerun for this change.
- Server tests cover queue deduplication and capacity, persistent restart recovery, terminal cancellation, completed-game import and cache reuse, rejected archive destinations, untrusted game selection, recent summaries, API errors and static serving. Job insertion is transactional; a filesystem lock prevents competing workers on one data volume.
- Frontend: **9 tests passed**, plus TypeScript, Oxlint and Prettier checks. Node-based contract checks cover all real fixtures, completion boundaries, legal variations, server review conversion, HTTP errors, saved-review restoration and polling cancellation without cancelling a server job. Rust formatting and Clippy with warnings denied also passed.
- Built the complete Docker image locally for **Linux ARM64** using full Stockfish 18 from commit `cb3d4ee9b47d0c5aae855b12379378ea1439675c`. The image is approximately 313 MB and includes corresponding Stockfish source and networks. AMD64 is configured in the native GitHub Actions build matrix; that platform's image was not built locally.
- Production-container smoke passed: full native engine identity, Rust API and static UI, missing-route 404s, no browser engine assets, UID 10001, read-only root with writable data volume, bundled source and healthy restart with persisted volume contents.
- Live end-to-end check: imported **40 rated rapid games for dre4success007**, all 40 replayed through the UI's chess library, and native Stockfish completed the latest **5** games at 150,000 scan / 1,000,000 confirmation nodes. Recreated the running container mid-analysis; saved results survived unchanged and the job resumed to completion. The resulting five-game canonical review passed CLI validation and frontend conversion.
- GitHub Actions YAML and embedded shell parsed successfully; actionlint reported **zero errors across all three workflows**. Publish and deploy are manual. No workflow was triggered, no image was pushed, and no remote deployment occurred. Deployment notes are maintained locally.
- Local Compose preview remains available at `http://localhost:8080` with the successful live review saved in the `tempo_tempo-data` volume. Browser visual/interaction QA and optional WebMCP registration remain unverified in this environment.
