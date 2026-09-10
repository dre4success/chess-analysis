import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import {
  api,
  mappedAnalyses,
  remainingReviewGames,
  watchReview,
  type ReviewJob,
} from '../lib/api.ts';
import { parseGame, type ApiGame, type GameAnalysis } from '../lib/chess.ts';
import type { SavedReview } from '../lib/review-data.ts';

const job: ReviewJob = {
  id: 7,
  username: 'dre4success007',
  pace: 'rapid',
  status: 'complete',
  created_at: '2026-09-09T12:00:00Z',
  progress: { completed: 1, total: 1, positions: 30 },
  message: 'Ready',
};
const importedGames = (
  JSON.parse(
    await readFile(
      new URL('../../tests/fixtures/rapid-12.json', import.meta.url),
      'utf8',
    ),
  ) as { games: ApiGame[] }
).games.toSorted((a, b) => b.end_time - a.end_time);
const games = importedGames.map((g) => parseGame(g, job.username)!);
const checked = (...indices: number[]): Record<string, GameAnalysis> =>
  Object.fromEntries(
    indices.map((i) => [
      games[i].id,
      {
        id: games[i].id,
        findings: [],
        source: 'native' as const,
        positions: 0,
      },
    ]),
  );

test('continuing repeated partial reviews preserves the original five-game scope', () => {
  const initial = {
    ...job,
    target_urls: [],
    progress: { completed: 2, total: 5, positions: 0 },
  };
  const remaining = remainingReviewGames(initial, games, checked(0, 1));
  assert.deepEqual(remaining, games.slice(2, 5));
  const child = {
    ...initial,
    target_urls: remaining.map((g) => g.id),
    progress: { completed: 1, total: 3, positions: 0 },
  };
  assert.deepEqual(
    remainingReviewGames(child, games, checked(0, 1, 2)),
    games.slice(3, 5),
  );
});

test('continuing a selected game does not add unrelated imported games', () => {
  const selected = {
    ...job,
    target_urls: [games[8].id],
    progress: { completed: 0, total: 1, positions: 0 },
  };
  assert.deepEqual(remainingReviewGames(selected, games, checked(0, 1, 2, 3, 4)), [
    games[8],
  ]);
  assert.deepEqual(remainingReviewGames(selected, games, checked(0, 1, 2, 3, 4, 8)), []);
  assert.deepEqual(
    remainingReviewGames({ ...selected, target_urls: ['missing-game'] }, games, {}),
    [],
  );
  assert.deepEqual(remainingReviewGames(null, games, {}), []);
});

test('initial targets use imported order without substituting for excluded games', () => {
  const initial: ReviewJob = {
    ...job,
    target_urls: [],
    data: {
      profile: { username: job.username },
      games: importedGames,
      pace: 'rapid',
      fetchedAt: job.created_at,
      archivesRead: 1,
    },
  };
  const replayable = games.filter((g) => g.id !== games[0].id);
  assert.deepEqual(remainingReviewGames(initial, replayable, {}), games.slice(1, 5));
});

test('reopening a completed review reads it once without submitting analysis', async (t) => {
  const requests: { url: string; method?: string }[] = [];
  t.mock.method(globalThis, 'fetch', async (url: string, options: RequestInit) => {
    requests.push({ url, method: options.method });
    return Response.json(job);
  });
  const seen = [];
  for await (const snapshot of watchReview(7, new AbortController().signal))
    seen.push(snapshot);
  assert.deepEqual(seen, [job]);
  assert.deepEqual(requests, [{ url: '/api/reviews/7', method: undefined }]);
});

test('disconnecting polling leaves the server job running', async (t) => {
  const urls: string[] = [];
  t.mock.method(globalThis, 'fetch', async (url: string) => {
    urls.push(url);
    return Response.json({ ...job, status: 'running' });
  });
  const controller = new AbortController();
  const stream = watchReview(7, controller.signal);
  assert.equal((await stream.next()).value?.status, 'running');
  controller.abort();
  await assert.rejects(stream.next(), { name: 'AbortError' });
  assert.deepEqual(urls, ['/api/reviews/7']);
});

test('API errors remain useful, including when a reverse proxy returns HTML', async (t) => {
  const mock = t.mock.method(globalThis, 'fetch', async () =>
    Response.json({ error: 'Player not found' }, { status: 404 }),
  );
  await assert.rejects(api('/reviews/99'), /Player not found/);
  mock.mock.mockImplementation(
    async () => new Response('<h1>Unavailable</h1>', { status: 502 }),
  );
  await assert.rejects(api('/reviews/99'), /server could not be reached/);
});

test('saved native findings become replayable UI data without a browser engine', async () => {
  const review = JSON.parse(
    await readFile(
      new URL('../../tests/fixtures/rapid-12-review.json', import.meta.url),
      'utf8',
    ),
  ) as SavedReview;
  const mapped = mappedAnalyses({ ...job, review });
  assert.equal(Object.keys(mapped).length, review.games.length);
  for (const game of review.games) {
    assert.equal(mapped[game.url].source, 'native');
    assert.equal(mapped[game.url].findings.length, game.findings.length);
    game.findings.forEach((finding, i) => {
      assert.equal(mapped[game.url].findings[i].beforeFen, finding.before_fen);
      assert.equal(mapped[game.url].findings[i].best, finding.best_san);
    });
  }
});
