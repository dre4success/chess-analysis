# Tempo

Review your Chess.com games with native Stockfish 18. Track ratings and openings, replay key moments, and explore better moves.

React/Vite frontend, Rust API, SQLite storage. Reviews continue on the server when you close the browser.

## Run

Requires Docker with Compose:

```sh
docker compose up --build -d --wait
```

Open [localhost:8080](http://localhost:8080). Configure the local port with [.env.example](.env.example).

Reviews persist in the `tempo_tempo-data` volume. `docker compose down -v` deletes them.

### Development without Docker

Requires Node.js 24, Rust 1.97.1 and Stockfish 18:

```sh
npm --prefix web ci
npm --prefix web run build
cargo run --release -- serve --engine /path/to/stockfish
```

Run `npm --prefix web run dev` in another terminal for frontend development.

## API

Base URL: `http://localhost:8080/api`.

| Method | Path                   | Purpose                               |
| ------ | ---------------------- | ------------------------------------- |
| `GET`  | `/health`              | Engine and runtime status             |
| `GET`  | `/reviews`             | Eight most recent review summaries    |
| `POST` | `/reviews`             | Start a review                        |
| `GET`  | `/reviews/{id}`        | Saved games, results and progress     |
| `POST` | `/reviews/{id}/cancel` | Stop a review; keep completed results |

```sh
curl http://localhost:8080/api/reviews \
  -H 'Content-Type: application/json' \
  -d '{"username":"your-username","pace":"rapid"}'
```

`pace` accepts `rapid` (default), `blitz` or `bullet`. Tempo imports up to 40 completed, rated standard games and reviews the latest five.

Poll the returned job's `/reviews/{id}` endpoint. Status is `queued`, `running`, `complete`, `cancelled` or `failed`. Results appear in `review`; imported games are in `data.games`. Open `/?review={id}` to resume in the UI.

To analyse additional games, post the same username and pace with `parent_id` and `game_urls` selected from that saved job. To cancel, post `{}` to `/reviews/{id}/cancel`.

## Deploy

[compose.prod.yaml](compose.prod.yaml) routes through an existing Traefik instance using network `web_proxy_net`, entrypoints `web` / `websecure`, and certificate resolver `myresolver`. It exposes container port 8080 to Traefik without publishing a host port.

On the server:

1. Copy `compose.prod.yaml` to the app directory as `docker-compose.yml`.
2. Copy [.env.production.example](.env.production.example) there as `.env`. Set `TEMPO_DOMAIN` and point its DNS at the server.
3. Put the published image reference in `tag.env`:

```dotenv
TEMPO_IMAGE=ghcr.io/dre4success/chess-analysis@sha256:<digest>
```

```sh
docker compose --env-file .env --env-file tag.env -f docker-compose.yml pull
docker compose --env-file .env --env-file tag.env -f docker-compose.yml up -d --no-build --wait
```

The instance has shared reviews and no built-in login. Set `TEMPO_TRAEFIK_MIDDLEWARES` in `.env` to use existing authentication, such as `auth@file`.

### GitHub Actions

Run **Publish Tempo image** to build and publish AMD64/ARM64 images. Run **Deploy Tempo** with the resulting image digest.

Configure the `production` environment with variables `DEPLOY_HOST`, `DEPLOY_USER`, `DEPLOY_PATH`, and optional `DEPLOY_PORT` (default `22`); add secrets `DEPLOY_SSH_KEY` and `DEPLOY_KNOWN_HOSTS`.

Deployment preserves the server's `.env` and review volume, and saves the successful image in `tag.env`. Deploy a previous digest to roll back. Private GHCR packages require a registry login on the server.

## CLI

```sh
cargo run -- review your-username --last 20 --output review-output
cargo run -- validate review-output/review.json
cargo run -- render review-output/review.json
```

Use `--pgn /path/to/games.pgn` for local games or `--offline` for cached archives. Outputs are `review.json`, `digest.md` and, after rendering, `report.html`.

## Checks

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
npm --prefix web test
npm --prefix web run typecheck
npm --prefix web run lint
python3 scripts/check-compose.py
```

Format with `cargo fmt` and `npm --prefix web run format` (Prettier).
