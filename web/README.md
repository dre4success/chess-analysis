# Tempo

A personal chess studio: enter a Chess.com username, import recent games, and explore the positions that matter. The graphite and lime interface puts the board first, with a real interactive example, rating and opening trends, move replay, and saved reviews.

## Run the complete app

From the repository root:

```sh
docker compose up --build -d --wait
```

Open http://localhost:8080. Rust serves both the static React UI and its API. The container contains the complete native Stockfish 18 engine and its full networks. There is no Node server or browser engine in the production runtime.

Production packaging is in `compose.prod.yaml` at the repository root. GitHub Actions is disabled pending deployment setup.

## Develop

Build the static assets once, then start Rust from the repository root:

```sh
npm --prefix web ci
npm --prefix web run build
cargo run -- serve --engine /absolute/path/to/stockfish
```

In another terminal, start the Vite development server:

```sh
npm --prefix web run dev
```

Vite proxies `/api`, `/example` and the favicon to Rust at `127.0.0.1:8080`. Fonts are bundled locally. `npm run build` emits only the frontend and example data into `dist/selfhost/`.

## Native analysis and saved reviews

`src/server/` owns the persistent SQLite queue. A single worker imports up to 40 completed, rated standard games from the latest eight active months and automatically reviews the latest five. Select any other imported game to review it on demand. Rapid, blitz and bullet are separate samples.

`src/pipeline.rs` is shared with the CLI. It uses the original completion checks, candidate selection, tactical detectors and validated review schema. Stockfish runs natively with 150,000 scan nodes, 1,000,000 confirmation nodes, one thread, 64 MiB hash, MultiPV 1, and cleared hash for each position. Findings show up to three confirmed drops of at least two pawns or changes to forced mate per game.

Closing the tab only stops polling. The server continues, and `?review=<id>` restores the review. The review shelf lists recent jobs. Cancelling explicitly stops the job at the next engine boundary and keeps finished games. After a server restart, interrupted jobs resume; completed games come from cache. Cache identity includes the Rust executable, Stockfish executable and metadata, username, game URL and exact PGN bytes.

The old WASM adapter, browser-engine scripts and Sites configuration remain as the earlier prototype. They are not used by the self-hosted application or copied into its production build. The existing compiled-core regression test remains for that adapter.

## Formatting and checks

```sh
npm run format         # Prettier
npm run format:check
npm run typecheck
npm run lint
npm test
npm run build
```

Prettier formats the application code. Oxlint checks the Vite application, with Next.js-only rules removed. Composite boards retain appropriate ARIA roles; replay is keyboard accessible. Root Rust code uses `cargo fmt`, tests and Clippy.

The tests cover source statistics, completion boundaries, legal finding replay, native review conversion, saved-review polling, disconnection and HTTP errors. Browser visual and interaction QA has not been performed in this environment. Optional WebMCP registration is feature-detected and remains unverified.

## Data and search limits

Chess.com may cache public data for up to 12 hours. Ratings are recorded game ratings. Invalid or unsupported PGNs are excluded and counted. A bounded engine search can miss mistakes; the interface does not invent an accuracy percentage or claim a structural detector proves a unique tactical cause. Engine lines start from individual positions; history-dependent repetition is not inferred.

Sources: [Chess.com public API](https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it), [Stockfish 18](https://github.com/official-stockfish/Stockfish/tree/sf_18), [chess.js](https://jhlywa.github.io/chess.js/).
