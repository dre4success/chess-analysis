import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { Chess } from 'chess.js';
import { parseGame, timeLabel, type ApiGame } from '../lib/chess.ts';
import type { RustEval, SavedReview } from '../lib/review-data.ts';

const preview = JSON.parse(
  await readFile(new URL('../assets/example-preview.json', import.meta.url), 'utf8'),
) as {
  url: string;
  fen: string;
  actual: string;
  best: string;
  move: number;
  username: string;
  white: ApiGame['white'];
  black: ApiGame['black'];
  timeControl: string;
  clockSecs: number;
  before: RustEval;
  after: RustEval;
};
const archive = JSON.parse(
  await readFile(new URL('../public/example/games.json', import.meta.url), 'utf8'),
) as { games: ApiGame[] };
const review = JSON.parse(
  await readFile(new URL('../public/example/review.json', import.meta.url), 'utf8'),
) as SavedReview;
const source = archive.games.find((game) => game.url === preview.url)!;
const parsed = parseGame(source, preview.username)!;
const finding = review.games
  .find((game) => game.url === preview.url)!
  .findings.find((f) => f.before_fen === preview.fen)!;

test('the welcome example shows the real game’s ten-minute control and player ratings', () => {
  assert.ok(source);
  assert.ok(parsed);
  assert.equal(preview.timeControl, source.time_control);
  assert.equal(preview.timeControl, '600');
  assert.equal(timeLabel(preview.timeControl), '10 min');
  for (const colour of ['white', 'black'] as const) {
    assert.equal(
      preview[colour].username.toLowerCase(),
      source[colour].username.toLowerCase(),
    );
    assert.equal(preview[colour].rating, source[colour].rating);
  }
  assert.equal(preview.white.rating, 1020);
  assert.equal(preview.black.rating, 916);
  assert.equal(preview.username, review.user);
});
test('the preview’s before, played and better boards all belong to the same verified moment', () => {
  assert.ok(finding);
  assert.equal(preview.move, Math.ceil(finding.ply / 2));
  assert.equal(preview.fen, parsed.moves[finding.ply - 1].before);
  assert.equal(new Chess(preview.fen).turn(), 'w');
  assert.equal(preview.actual, finding.actual_san);
  assert.equal(preview.best, finding.best_san);
  const actual = new Chess(preview.fen).move(preview.actual);
  const best = new Chess(preview.fen).move(preview.best);
  assert.equal(actual.after, finding.after_fen);
  assert.equal(actual.lan, finding.actual_uci);
  assert.equal(best.lan, finding.principal_variation_uci[0]);
});
test('preview evaluation and remaining clock come from that finding, not duplicated editorial estimates', () => {
  assert.deepEqual(preview.before, finding.eval_before);
  assert.deepEqual(preview.after, finding.eval_after);
  assert.equal(preview.clockSecs, finding.clock_secs);
});
