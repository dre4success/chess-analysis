import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { Chess, type Move } from 'chess.js';
import { parseGame, timeLabel, type ApiGame, type Pace } from '../lib/chess.ts';
import { convertFinding, type RustEval, type SavedReview } from '../lib/review-data.ts';

const profile = JSON.parse(
  await readFile(new URL('../assets/example-profile.json', import.meta.url), 'utf8'),
) as { username: string; name: string; shortName: string; pace: Pace };
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
) as Omit<SavedReview, 'games'> & {
  mode: string;
  engine: { name: string };
  games: (SavedReview['games'][number] & { input_pgn_sha256: string })[];
};
const source = archive.games.find((game) => game.url === preview.url);
const parsed = source ? parseGame(source, preview.username) : null;
const finding = review.games
  .find((game) => game.url === preview.url)
  ?.findings.find((f) => f.before_fen === preview.fen);

test('the public example consistently features Hikaru’s blitz games, without the previous personal demo', () => {
  assert.equal(profile.username, 'hikaru');
  assert.equal(profile.name, 'Hikaru Nakamura');
  assert.equal(profile.shortName, 'Hikaru');
  assert.equal(profile.pace, 'blitz');
  assert.equal(preview.username, profile.username);
  assert.equal(review.user, profile.username);
  assert.equal(archive.games.length, 12);
  assert.equal(new Set(archive.games.map((game) => game.url)).size, 12);
  for (const game of archive.games) {
    assert.equal(game.time_class, profile.pace);
    assert.ok(
      [game.white.username, game.black.username].some(
        (name) => name.toLowerCase() === profile.username,
      ),
    );
  }
  assert.doesNotMatch(
    JSON.stringify({ profile, preview, archive, review }),
    /dre4success007/i,
  );
});

test('all public sample games replay legally and their engine findings belong to their original moves', () => {
  assert.equal(review.mode, 'verified');
  assert.equal(review.engine.name, 'Stockfish');
  assert.equal(review.games.length, archive.games.length);
  assert.deepEqual(
    new Set(review.games.map((game) => game.url)),
    new Set(archive.games.map((game) => game.url)),
  );
  // CLI --pgn records the checksum of its complete input, including all games.
  const inputPgn = `${archive.games.map((game) => game.pgn.trimEnd()).join('\n\n')}\n`;
  const inputChecksum = createHash('sha256').update(inputPgn).digest('hex');
  for (const raw of archive.games) {
    const game = parseGame(raw, profile.username);
    assert.ok(game, `Cannot replay sample game ${raw.url}`);
    const checked = review.games.find((entry) => entry.url === raw.url)!;
    assert.equal(checked.input_pgn_sha256, inputChecksum);
    for (const moment of checked.findings) {
      const played: Move | undefined = game.moves[moment.ply - 1];
      assert.ok(played, `Finding outside game ${raw.url}, ply ${moment.ply}`);
      assert.equal(played.color, game.colour);
      assert.equal(played.before, moment.before_fen);
      assert.equal(played.after, moment.after_fen);
      assert.equal(played.lan, moment.actual_uci);
      assert.equal(played.san, moment.actual_san);
      const converted = convertFinding(moment);
      assert.ok(converted.pv.length > 0);
      assert.equal(moment.principal_variation_uci[0], moment.best_uci);
      assert.equal(converted.pv[0], moment.best_san);
      assert.ok(Number.isFinite(converted.before.value));
      assert.ok(Number.isFinite(converted.after.value));
      for (const [fen, line] of [
        [converted.beforeFen, converted.pv],
        [converted.afterFen, converted.refutationPv ?? []],
      ] as const) {
        const board = new Chess(fen);
        for (const san of line) assert.ok(board.move(san));
      }
    }
  }
});

test('the welcome example shows its real source game’s time control and player ratings', () => {
  assert.ok(source);
  assert.ok(parsed);
  assert.equal(preview.timeControl, source.time_control);
  assert.equal(preview.timeControl, '180');
  assert.equal(timeLabel(preview.timeControl), '3 min');
  for (const colour of ['white', 'black'] as const) {
    assert.equal(
      preview[colour].username.toLowerCase(),
      source[colour].username.toLowerCase(),
    );
    assert.equal(preview[colour].rating, source[colour].rating);
  }
  assert.equal(preview.username, review.user);
});

test('the preview’s before, played and better boards all belong to the same verified moment', () => {
  assert.ok(finding);
  assert.ok(parsed);
  assert.equal(preview.move, Math.ceil(finding.ply / 2));
  assert.equal(preview.fen, parsed.moves[finding.ply - 1].before);
  assert.equal(new Chess(preview.fen).turn(), parsed.colour);
  assert.equal(preview.actual, finding.actual_san);
  assert.equal(preview.best, finding.best_san);
  const actual = new Chess(preview.fen).move(preview.actual);
  const best = new Chess(preview.fen).move(preview.best);
  assert.equal(actual.after, finding.after_fen);
  assert.equal(actual.lan, finding.actual_uci);
  assert.equal(best.lan, finding.principal_variation_uci[0]);
});

test('preview evaluation and remaining clock come from that finding, not duplicated editorial estimates', () => {
  assert.ok(finding);
  assert.deepEqual(preview.before, finding.eval_before);
  assert.deepEqual(preview.after, finding.eval_after);
  assert.equal(preview.clockSecs, finding.clock_secs);
});
