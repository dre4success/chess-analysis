# chess-review

A Rust CLI for reviewing **completed** chess games with Stockfish. It produces a
canonical `review.json`, a concise `digest.md` for coaching, and an offline HTML
report with before/after boards. Repeated patterns rank by games affected.

## Tempo web interface

The `web/` app adds a personal chess studio: enter a Chess.com username, import recent
completed games, explore a featured position, track rating/opening trends, and replay
key moments. Rust serves the static React UI and API. Full native Stockfish 18 evaluates
positions through the original shared Rust pipeline. Reviews run in a persistent SQLite
queue, continue after closing the tab and resume after server restarts.

```sh
docker compose up --build -d --wait
```

Open http://localhost:8080. Docker includes Rust, the static UI and the complete native
Stockfish engine. Prettier formats the frontend; `cargo fmt` formats Rust.
See [web development](web/README.md) for the UI. Deployment workflows are prepared
in `.github/workflows/` for later setup.

Production uses [compose.prod.yaml](compose.prod.yaml) with your existing Traefik
on `web_proxy_net`, `web`/`websecure` entrypoints and `myresolver` TLS resolver.
It routes to Tempo's internal port 8080 without publishing a host port.

GitHub Actions is disabled for now. Workflow definitions are saved for later manual
use; pushes and pull requests do not trigger builds, publishing or deployment.

## Run

Requires a current Rust toolchain (edition 2024) and Stockfish on PATH, or an explicit
`--engine /absolute/path/to/stockfish`. Normal tests do not require Stockfish;
process integration tests use Python 3 on Unix.

```sh
cargo run -- review dre4success007 --last 20 --output review-output
cargo run -- render review-output/review.json
cargo run -- validate review-output/review.json
```

The network command selects the most recent **rated rapid** games. Fetches are
serial. `--cache` chooses the cache directory (default `.chess-review-cache`);
`--offline` uses cached archives and months without network requests. Closed months
are immutable. Missing months and HTTP errors fail the run; rejected games are
reported to stderr and `skipped.txt`. A rejected entry is not silently replaced by
an older game, so the report states the actual number analysed.

Local PGNs can contain multiple games; the last matching games in file order are
selected. The input hash identifies the exact PGN file (all games share that hash),
and local fallback URLs include the game index.

```sh
cargo run -- review dre4success007 --pgn tests/fixtures/rapid-12.pgn --last 12 \
  --engine /opt/homebrew/bin/stockfish --output review-output/local
```

`review` writes JSON and digest. `render` regenerates both digest and HTML **from
validated JSON alone**, without fetching or opening an engine. Writes use temporary
files and atomic replacement. Artifacts are individually atomic, not a multi-file
transaction; if rendering fails, rerun `render` using the canonical JSON.

## Analysis contract

- Local completion policy requires legal replay, standard chess, a finished Result
  header and matching movetext marker. Local exports may omit Termination. API
  entries additionally require a past `end_time`, matching source results/players,
  confirmed Termination, standard rules and no ongoing URL signals. Finished
  `/game/live/…` URLs are valid. All entries are guarded before opening Stockfish.
- Defaults: 150,000 scan nodes, 1,000,000 confirmation nodes, 200 cp threshold;
  configurable with `--scan-nodes`, `--deep-nodes`, `--threshold-cp`. Scan loss at
  half the threshold shortlists; only deeper results decide. At most three findings
  per game survive confirmation. This shortlist may miss mistakes invisible at the
  scan budget. No depth or wall-time search limits; the timeout is a failure limit.
- Threads 1, Hash 64 MiB, MultiPV 1, clear hash before every position. Each position
  is searched from its FEN; history-dependent repetition claims are not inferred
  by the engine. Terminal board positions, automatic fivefold/75-move draws and final claimed threefold/50-move draws are handled directly.
- When node exhaustion interrupts an iteration, a final UCI `bestmove` can carry
  only a score bound. The reported score and alternative come together from the
  **last exact-score PV**, not the interrupted iteration. Neither that alternative
  nor the final UCI best move is condemned. This policy is recorded in metadata.
- Centipawn arithmetic never includes mate sentinels. Maintaining a forced win or
  shortening an already-lost forced mate is not flagged as a new mistake.
- Six deterministic detectors explain confirmed candidates; unmatched cases use
  `engine-verified-mistake`. Precedence: missed mate, line opened, defender left,
  attacked piece ignored, capture cost, line onto. Other matches are retained.
  Exchange evidence examines legal captures on one square; it is not a general
  tactical search. Labels describe board relationships, not a proven unique cause.
- `%clk` means remaining time. Elapsed time is previous remaining + increment −
  current remaining. Missing or inconsistent annotations stay unknown; `%timestamp`
  is not interpreted. Clock percentage uses base time plus earned increments.
  Clock bands use time remaining after the move. Phase bands are explicit ply ranges.
- Every persisted actual move, best move, SAN, FEN and PV is validated. Pattern
  counts and references are checked. Imported metadata is escaped in reports.

Engine version, executable digest, network identities, UCI options, budgets,
threshold, search order and platform are recorded. Same-machine stability is
measured; cross-binary or cross-platform byte equality is not promised.

## Development and acceptance

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
STOCKFISH=/opt/homebrew/bin/stockfish cargo test -- --ignored --nocapture
```

See `VALIDATION.md` for observed results and `PLAN.md` for the original roadmap.
The Tempo web rebuild was requested on 2026-09-08. The earlier Stage 11
semantic-label usefulness review remains a separate human judgment. Release still requires a human to inspect semantic labels
and identify a practice priority worth acting on. The software does not substitute
an engine verdict for that judgment.

Protocol references: [Stockfish UCI implementation](https://github.com/official-stockfish/Stockfish/blob/master/src/uci.cpp)
and [Chess.com PubAPI guide](https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it).
