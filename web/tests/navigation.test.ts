import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readRoute, routeUrl } from '../lib/navigation.ts';

test('a shared position restores its game, finding and complete variation cursor', () => {
  const route = {
    view: 'review' as const,
    reviewId: 7,
    gameId: '174030627562',
    position: { ply: 26, mode: 'refutation' as const, findingPly: 26, linePly: 12 },
  };
  assert.deepEqual(readRoute(routeUrl(route).split('?')[1]), {
    ...route,
    example: undefined,
  });
  assert.equal(
    readRoute('?example=1&game=123&ply=60&mode=better&finding=61&line=11').position
      ?.linePly,
    11,
  );
});
test('route input cannot introduce an external game, invalid cursor or unsafe job id', () => {
  const route = readRoute(
    '?review=90071992547409999&game=https://other.example&ply=-1&mode=bogus',
  );
  assert.equal(route.reviewId, undefined);
  assert.equal(route.gameId, undefined);
  assert.equal(route.position, undefined);
  assert.equal(route.view, 'welcome');
  assert.equal(readRoute('?review=1&game=123&ply=99999').position, undefined);
});
test('library and example routes survive refresh without submitting analysis', () => {
  assert.equal(readRoute('?view=library').view, 'library');
  assert.equal(routeUrl({ view: 'overview', example: true }), '/?example=1');
});
