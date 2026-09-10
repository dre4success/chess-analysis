import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import {
  parseGame,
  type ApiGame,
  type Finding,
  type GameAnalysis,
} from '../lib/chess.ts';
import { studyPriority, themesFor } from '../lib/coaching.ts';
import { convertFinding, type SavedReview } from '../lib/review-data.ts';
import { gameKey, readRoute, routeUrl } from '../lib/navigation.ts';
import { findingPosition } from '../lib/study.ts';

const archive = JSON.parse(
  await readFile(new URL('../public/example/games.json', import.meta.url), 'utf8'),
) as { games: ApiGame[] };
const saved = JSON.parse(
  await readFile(new URL('../public/example/review.json', import.meta.url), 'utf8'),
) as SavedReview;
const base = parseGame(archive.games[0], saved.user)!;
const games = ['101', '102', '103', '104'].map((id) => ({
  ...base,
  id: `https://www.chess.com/game/live/${id}`,
}));
const baseline = convertFinding(saved.games[0].findings[0]);
function moment(
  classification: string | undefined,
  ply: number,
  before = 50,
  after = -250,
): Finding {
  return {
    ...baseline,
    classification,
    ply,
    before: { type: 'cp', value: before },
    after: { type: 'cp', value: after },
    loss: before - after,
  };
}
const analysis = (index: number, findings: Finding[]): GameAnalysis => ({
  id: games[index].id,
  findings,
  source: 'native',
  positions: 30,
});

test('a recurring habit counts distinct affected games and only checked games in its denominator', () => {
  const analyses = {
    [games[0].id]: analysis(0, [
      moment('line-opened', 11),
      moment('line-opened', 25),
      moment('line-opened', 39),
    ]),
    [games[1].id]: analysis(1, [moment('line-opened', 21)]),
    [games[2].id]: analysis(2, []),
    'out-of-sample': { ...analysis(3, [moment('line-opened', 15)]), id: 'out-of-sample' },
  };
  const [theme] = themesFor(games, analyses);
  assert.equal(theme.affected, 2);
  assert.equal(theme.reviewed, 3);
  assert.equal(theme.examples.length, 4);
  assert.deepEqual(
    new Set(theme.examples.map((example) => example.game.id)),
    new Set([games[0].id, games[1].id]),
  );
});
test('repetition across games outranks several incidents in only one game', () => {
  const themes = themesFor(games, {
    [games[0].id]: analysis(0, [
      moment('line-opened', 11),
      moment('line-opened', 25),
      moment('line-opened', 39),
    ]),
    [games[1].id]: analysis(1, [moment('defender-left', 21)]),
    [games[2].id]: analysis(2, [moment('defender-left', 35)]),
  });
  assert.equal(themes[0].classification, 'defender-left');
  assert.equal(themes[0].affected, 2);
  assert.equal(themes[1].affected, 1);
});
test('unclassified findings remain reviewable without becoming a fabricated coaching theme', () => {
  const generic = moment('engine-verified-mistake', 11);
  const analyses = {
    [games[0].id]: analysis(0, [
      generic,
      moment(undefined, 25),
      moment('unknown-new-detector', 39),
    ]),
  };
  assert.deepEqual(themesFor(games, analyses), []);
  assert.equal(studyPriority(games, analyses)?.game.id, games[0].id);
  assert.equal(studyPriority(games, {}), undefined);
});
test('a competitive decision outranks allowing mate after a hopeless position', () => {
  const hopeless: Finding = {
    ...moment('attacked-piece-ignored', 26, -805),
    after: { type: 'mate', value: -1 },
    loss: null,
  };
  const competitive = moment('engine-verified-mistake', 41, 30, -210);
  const priority = studyPriority(games, {
    [games[0].id]: analysis(0, [hopeless]),
    [games[1].id]: analysis(1, [competitive]),
  });
  assert.equal(priority?.game.id, games[1].id);
  assert.equal(priority?.finding, competitive);
});
test('a recurring theme uses its competitive example instead of its newer hopeless collapse', () => {
  const hopeless: Finding = {
    ...moment('attacked-piece-ignored', 26, -805),
    after: { type: 'mate', value: -1 },
    loss: null,
  };
  const competitive = moment('attacked-piece-ignored', 41, 30, -210);
  const priority = studyPriority(games, {
    [games[0].id]: analysis(0, [hopeless]),
    [games[1].id]: analysis(1, [competitive]),
  });
  assert.equal(priority?.game.id, games[1].id);
  assert.equal(priority?.finding, competitive);
});
test('all lesson comparison modes preserve their board context in a saved-position URL', () => {
  for (const mode of ['before', 'played', 'better', 'refutation'] as const) {
    const position = {
      ...findingPosition(baseline, mode),
      ...(mode === 'better' || mode === 'refutation' ? { linePly: 0 } : {}),
    };
    const url = routeUrl({
      view: 'review',
      example: true,
      gameId: gameKey(saved.games[0].url),
      position,
    });
    const route = readRoute(new URL(url, 'http://localhost').search);
    assert.equal(route.view, 'review');
    assert.equal(route.gameId, gameKey(saved.games[0].url));
    assert.equal(route.position?.mode, position.mode);
    assert.equal(route.position?.ply, position.ply);
    assert.equal(route.position?.findingPly, position.findingPly);
    assert.equal(route.position?.linePly, position.linePly);
  }
});
