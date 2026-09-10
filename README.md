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
| `GET`  | `/health`              | Engine, storage and review queue status |
| `GET`  | `/reviews?usernames=alice,bob` | Reviews for explicitly selected players, newest first  |
| `POST` | `/reviews`             | Start a review                        |
| `GET`  | `/reviews/{id}`        | Saved games, results and progress     |
| `GET`  | `/reviews/{id}/digest` | Download the saved coaching digest as Markdown |
| `POST` | `/reviews/{id}/cancel` | Stop a review; keep completed results |

```sh
curl http://localhost:8080/api/reviews \
  -H 'Content-Type: application/json' \
  -d '{"username":"your-username","pace":"rapid"}'
```

`pace` accepts `rapid` (default), `blitz` or `bullet`. Tempo imports up to 40 completed, rated standard games and reviews the latest five.

Poll the returned job's `/reviews/{id}` endpoint. Status is `queued`, `running`, `complete`, `cancelled` or `failed`. Results appear in `review`; imported games are in `data.games`. Open `/?review={id}` to resume in the UI.

To analyse additional games, post the same username and pace with `parent_id` and `game_urls` selected from that saved job. To cancel, post `{}` to `/reviews/{id}/cancel`.

Review history is explicitly scoped: use `/reviews?usernames=your-username` for up to eight summaries, or `/reviews?usernames=alice,bob&offset=8&limit=20` to browse older reviews for those players. Missing or blank `usernames` returns an empty history. The comma-separated list accepts up to 100 unique, case-insensitive usernames; `limit` accepts 1–100. Responses include `offset`, `limit`, `total` and `has_more`, plus each review's imported game count, game date range and finding count when available. The digest becomes available after the first game finishes; an unfinished review exports the completed games and is marked partial.

Health includes queued, running and failed job counts, the current job ID and the latest failed job's error. It returns HTTP 503 when storage is unavailable or the server is shutting down. Browser changes must come from the instance's own origin; keep the public `Host` header when configuring a reverse proxy.

An archive cached while its month was active is refreshed after that month ends. Copies fetched at least one day after month end can be reused permanently; the CLI's offline mode still uses the available cached copy.

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

Each browser remembers up to 100 explicitly looked-up usernames in local storage. Welcome and the library request reviews only for those players; a new browser starts empty, and viewing the example or a direct review link does not add a player. Clearing site data clears that browser’s lookup list, not the saved reviews. Searching the username again makes its saved reviews available.

The public sample is a fixed, engine-checked selection of Hikaru Nakamura’s public blitz games. It is labeled throughout the studio; the homepage position opens directly in the study room. **Find my games** returns to the username form without importing the sample player. See the [sample provenance](web/public/example/README.md) for source data and reproduction instructions.

Saved reviews and analysis caches remain shared on the server, with no built-in login: username filtering organizes discovery, not access control. Anyone able to reach the instance can deliberately look up a public profile or open a known review link. Set `TEMPO_TRAEFIK_MIDDLEWARES` in `.env` to use existing authentication, such as `auth@file`.

### GitHub Actions

Every push to `main` runs **Publish Tempo image**: Rust and UI checks, a production-container smoke test, native AMD64/ARM64 builds and engine smoke tests, then publication to GHCR. The workflow automatically calls **Deploy Tempo** with the tested image's immutable digest and the same commit's Compose definition. The deployment summary includes the healthy image and public URL.

Production deployments run one at a time. Before an automatic deployment starts, it checks that its commit is still the head of `main`; superseded builds are skipped. A manually started **Publish Tempo image** only checks and publishes. Run **Deploy Tempo** manually with a published digest to deploy a chosen version or roll back.

Configure the `production` environment with variables `DEPLOY_HOST`, `DEPLOY_USER`, `DEPLOY_PATH`, and optional `DEPLOY_PORT` (default `22`); add secrets `DEPLOY_SSH_KEY` and `DEPLOY_KNOWN_HOSTS`.

Deployment preserves the server's `.env` and review volume, and saves the successful image in `tag.env`. Deploy a previous digest to roll back. Private GHCR packages require a registry login on the server.

A failed deployment does not roll back automatically. If the new container fails its health check, it may remain installed while `tag.env` still names the last successful image. Rerun **Deploy Tempo** with the previous digest, or run the server's Compose `pull` and `up --wait` commands above using that saved `tag.env`. The workflow also replaces `docker-compose.yml` before starting the image; restore a compatible Compose definition from a backup if its settings caused the failure.

### Back up and restore reviews

SQLite may hold committed writes in `tempo.sqlite3-wal`, so copying only the main database while Tempo runs is not a safe backup. Stop the writer and copy all of `/data`, including any WAL files and cached archives. In Bash, from the production deployment directory:

```sh
tempo_compose() {
  docker compose --env-file .env --env-file tag.env -f docker-compose.yml "$@"
}
(
  set -euo pipefail
  backup_dir="$PWD/tempo-backup-$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -m 700 "$backup_dir"
  tempo_compose stop tempo
  tempo_compose cp tempo:/data/. "$backup_dir/data"
  cp .env tag.env docker-compose.yml "$backup_dir/"
  tempo_compose start --wait tempo
  printf 'Backup saved to %s\n' "$backup_dir"
)
```

Keep that directory off the server as well. It includes deployment settings and may include authentication configuration. Interrupted reviews resume when the service starts again.

To restore, use a compatible saved image and Compose configuration and keep Tempo stopped until the copy completes. The following keeps a separate copy of the current data, removes the old database and its WAL sidecars, then restores the chosen snapshot through a one-off container running as Tempo's UID 10001. The original volume is retained, and the restored history replaces its current history. Set `backup_dir` to the backup you intend to restore; use the `tempo_compose` function above.

```sh
(
  set -euo pipefail
  backup_dir="/absolute/path/to/tempo-backup-TIMESTAMP"
  test -s "$backup_dir/data/tempo.sqlite3"
  before_restore="$PWD/tempo-before-restore-$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -m 700 "$before_restore"
  tar -C "$backup_dir/data" -cf "$before_restore/restore-data.tar" .
  tempo_compose stop tempo
  tempo_compose cp tempo:/data/. "$before_restore/data"
  tempo_compose run --rm --no-deps -T --entrypoint sh tempo \
    -c 'rm -f /data/tempo.sqlite3 /data/tempo.sqlite3-wal /data/tempo.sqlite3-shm'
  tempo_compose run --rm --no-deps -T --entrypoint tar tempo \
    --extract --file=- --directory=/data --no-same-owner < "$before_restore/restore-data.tar"
  tempo_compose up -d --no-build --wait tempo
)
```

A failure in the copy or restore steps exits the block before restarting Tempo. Resolve it before allowing new writes.

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
bash scripts/smoke-container.sh tempo:local
```

The container smoke test checks the API and assets, reviews a short completed PGN with the bundled native Stockfish using fixed node budgets, validates and renders its findings, then verifies the saved review survives a restart. Its game analysis is offline.

Format with `cargo fmt` and `npm --prefix web run format` (Prettier).
