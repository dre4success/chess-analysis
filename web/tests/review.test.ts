import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { Chess } from 'chess.js';
import {
  parseGame,
  summarise,
  normaliseUsername,
  USERNAME_PATTERN,
  isCompleted,
  type ApiGame,
} from '../lib/chess.ts';
import {
  RustReview,
  convertFinding,
  type CoreExports,
  type SavedReview,
} from '../lib/rust-core.ts';
import { exactInfo } from '../lib/engine.ts';
const archive = JSON.parse(
  await readFile(new URL('../../tests/fixtures/rapid-12.json', import.meta.url), 'utf8'),
) as { games: ApiGame[] };
const saved = JSON.parse(
  await readFile(
    new URL('../../tests/fixtures/rapid-12-review.json', import.meta.url),
    'utf8',
  ),
) as SavedReview;
const user = 'dre4success007';
test('username form pattern compiles with HTML Unicode sets and accepts hyphens', () => {
  const pattern = new RegExp(`^(?:${USERNAME_PATTERN})$`, 'v');
  for (const name of ['Dre4success007', 'Dre_4-success', 'a'.repeat(64)]) {
    assert.equal(pattern.test(name), true);
    assert.equal(normaliseUsername(name), name.toLowerCase());
  }
  for (const name of ['', 'a'.repeat(65), '../root', 'user name', 'user@name']) {
    assert.equal(pattern.test(name), false);
    assert.throws(() => normaliseUsername(name));
  }
});
test('all recorded games legally replay and statistics match source results', () => {
  const games = archive.games.map((g) => parseGame(g, user));
  assert.ok(games.every(Boolean));
  const stats = summarise(games.filter((g) => g !== null));
  assert.equal(
    stats.wins,
    archive.games.filter(
      (g) =>
        (g.white.username.toLowerCase() === user ? g.white : g.black).result === 'win',
    ).length,
  );
  assert.equal(stats.wins + stats.draw + stats.loss, 12);
  assert.equal(
    stats.colours.reduce((n, c) => n + c.games, 0),
    12,
  );
  assert.equal(
    stats.openings.reduce((n, o) => n + o.games, 0),
    12,
  );
});
test('finished-game boundary rejects invalid, unfinished and unrelated games', () => {
  assert.equal(normaliseUsername(' Dre4success007 '), user);
  assert.throws(() => normaliseUsername('../../secrets'));
  const g = archive.games[0];
  assert.equal(isCompleted({ ...g, end_time: Date.now() / 1000 + 3600 }, user), false);
  assert.equal(
    isCompleted({ ...g, url: 'https://example.com/game/live/123' }, user),
    false,
  );
  assert.equal(isCompleted(g, 'unrelated'), false);
  assert.equal(
    parseGame({ ...g, pgn: g.pgn.replace(/\[Result "[^"]+"\]/, '[Result "*"]') }, user),
    null,
  );
  assert.equal(parseGame({ ...g, pgn: g.pgn.replace('1. d4', '1. Ke8') }, user), null);
});
test('every saved finding and alternative line is legal in the web viewer', () => {
  for (const game of saved.games) {
    for (const raw of game.findings) {
      const finding = convertFinding(raw);
      const board = new Chess(finding.beforeFen);
      assert.equal(board.move(finding.actual).after, finding.afterFen);
      const best = new Chess(finding.beforeFen);
      assert.equal(best.move(finding.best).lan, finding.bestUci);
      const line = new Chess(finding.beforeFen);
      for (const san of finding.pv) assert.ok(line.move(san));
      assert.ok(finding.pv.length > 0);
      assert.ok(Number.isFinite(finding.before.value));
      assert.ok(Number.isFinite(finding.after.value));
    }
  }
});
test('interrupted search bounds cannot replace an exact evaluation', () => {
  assert.equal(exactInfo('info score cp 800 lowerbound pv e2e4'), null);
  assert.equal(exactInfo('info string score cp 800 pv e2e4'), null);
  assert.equal(exactInfo('info multipv 2 score cp 800 pv e2e4'), null);
  assert.deepEqual(exactInfo('info depth 7 score cp 30 pv e2e4 e7e5'), {
    score: { type: 'cp', value: 30 },
    pv: ['e2e4', 'e7e5'],
  });
});
test('compiled Rust validates completion and requests the original review budgets', async () => {
  const module = await WebAssembly.compile(
    await readFile(new URL('../public/rust/chess_review.wasm', import.meta.url)),
  );
  const instance = await WebAssembly.instantiate(module, {});
  const rust = new RustReview(instance.exports as unknown as CoreExports);
  assert.throws(
    () =>
      rust.call({
        type: 'start',
        game: { ...archive.games[0], end_time: 2000000000 },
        username: user,
        now: 1800000000,
      }),
    /completion|unsupported/,
  );
  const step = rust.call({
    type: 'start',
    game: archive.games[0],
    username: user,
    now: 1800000000,
  });
  assert.ok(step.requests.length > 0);
  assert.equal(step.findings.length, 0);
  assert.ok(step.requests.some((r) => r.nodes === 150000));
  assert.throws(() => rust.call({ type: 'continue', responses: [] }), /incomplete/);
});
