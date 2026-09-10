import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ReviewJob } from '../lib/api.ts';
import { stableReviewDisplay } from '../lib/review-snapshot.ts';

const snapshot: ReviewJob = {
  id: 7,
  username: 'player',
  pace: 'rapid',
  status: 'running',
  created_at: '2026-09-10T10:00:00Z',
  message: 'Checking game 2',
  progress: { completed: 1, total: 5, positions: 100 },
  data: {
    profile: { username: 'Player' },
    pace: 'rapid',
    fetchedAt: '2026-09-10T10:00:00Z',
    games: [],
    archivesRead: 2,
  },
  review: {
    user: 'player',
    generated: '2026-09-10T10:00:00Z',
    games: [{ url: 'https://www.chess.com/game/live/1', findings: [] }],
  },
};

test('progress and completion snapshots do not rebuild unchanged display content', () => {
  for (const status of ['running', 'complete', 'cancelled'] as const) {
    const next = {
      ...structuredClone(snapshot),
      status,
      message: 'Different progress',
      progress: { completed: 1, total: 5, positions: 200 },
    };
    assert.equal(stableReviewDisplay(snapshot, next), snapshot);
  }
});
test('a newly completed game updates findings without replaying the same imported PGNs', () => {
  const next = structuredClone(snapshot);
  next.review!.games.push({ url: 'https://www.chess.com/game/live/2', findings: [] });
  const display = stableReviewDisplay(snapshot, next)!;
  assert.notEqual(display, snapshot);
  assert.equal(display.data, snapshot.data);
  assert.equal(display.review, next.review);
  assert.equal(display.review!.games.length, 2);
});
test('changed import content updates even when the server timestamp is unchanged', () => {
  const next = structuredClone(snapshot);
  next.data!.warning = 'One archive could not be read.';
  const display = stableReviewDisplay(snapshot, next)!;
  assert.equal(display.data, next.data);
  assert.equal(display.review, snapshot.review);
  assert.match(display.data!.warning!, /archive/);
});
test('a different saved review cannot inherit another review’s profile or findings', () => {
  const queued = {
    ...snapshot,
    id: 8,
    status: 'queued' as const,
    data: undefined,
    review: undefined,
  };
  assert.equal(stableReviewDisplay(snapshot, queued), null);
  const next = { ...structuredClone(snapshot), id: 8 };
  assert.equal(stableReviewDisplay(snapshot, next), next);
  assert.notEqual(stableReviewDisplay(snapshot, next)!.data, snapshot.data);
  assert.equal(
    stableReviewDisplay(snapshot, { ...queued, id: 7, username: 'different-player' }),
    null,
  );
});
test('an explicit child review preserves its parent sample while queued without borrowing another player', () => {
  const queued = {
    ...snapshot,
    id: 8,
    status: 'queued' as const,
    data: undefined,
    review: undefined,
  };
  const child = stableReviewDisplay(snapshot, queued, snapshot.id)!;
  assert.equal(child.id, 8);
  assert.equal(child.data, snapshot.data);
  assert.equal(child.review, snapshot.review);
  assert.equal(stableReviewDisplay(child, queued), child);
  assert.equal(
    stableReviewDisplay(snapshot, { ...queued, username: 'other' }, snapshot.id),
    null,
  );
  assert.equal(
    stableReviewDisplay(snapshot, { ...queued, pace: 'blitz' }, snapshot.id),
    null,
  );
});
test('same-review interruption retains usable results but never restores a cleared session', () => {
  const stopped = {
    ...snapshot,
    status: 'cancelled' as const,
    data: undefined,
    review: undefined,
  };
  assert.equal(stableReviewDisplay(snapshot, stopped), snapshot);
  assert.equal(stableReviewDisplay(null, stopped), null);
});
