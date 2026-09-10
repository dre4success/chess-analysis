import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { Chess } from 'chess.js';
import { convertFinding, type SavedReview } from '../lib/review-data.ts';
import {
  boardDescription,
  evaluationChange,
  findingPosition,
  normaliseStudyPosition,
  positionAtPly,
  variationFrames,
} from '../lib/study.ts';

const saved = JSON.parse(
  await readFile(new URL('../public/example/review.json', import.meta.url), 'utf8'),
) as SavedReview;
const find = (gameId: string, ply: number) => {
  const raw = saved.games
    .find((game) => game.url.endsWith(gameId))!
    .findings.find((finding) => finding.ply === ply)!;
  return { raw, finding: convertFinding(raw) };
};

test('the example alternative reaches its promotion and both complete mating lines', () => {
  const promotion = find('173928199510', 61);
  assert.equal(promotion.finding.pv[10], 'g8=Q');
  assert.equal(promotion.finding.pv.length, promotion.raw.principal_variation_uci.length);
  for (const ply of [72, 76]) {
    const { finding, raw } = find('173984600836', ply);
    const frames = variationFrames(finding.beforeFen, finding.pv);
    assert.equal(frames.length, raw.principal_variation_uci.length + 1);
    assert.equal(new Chess(frames.at(-1)!.fen).isCheckmate(), true);
  }
});

test('a missed winning mate and an allowed opponent mate have distinct descriptions', () => {
  const missed = find('173984600836', 72).finding;
  const allowed = find('174030627562', 26).finding;
  assert.deepEqual(evaluationChange(missed.before, missed.after), {
    kind: 'missed-mate',
    label: 'Missed forced mate',
  });
  assert.deepEqual(evaluationChange(allowed.before, allowed.after), {
    kind: 'allowed-mate',
    label: 'Allowed forced mate',
  });
  assert.equal(
    evaluationChange({ type: 'cp', value: 0 }, { type: 'mate', value: -0 }).kind,
    'allowed-mate',
  );
  assert.deepEqual(
    evaluationChange({ type: 'cp', value: 150 }, { type: 'cp', value: -150 }),
    { kind: 'pawns', centipawns: 300, label: '−3.0 pawns' },
  );
});

test('scoresheet navigation selects the matching explanation or leaves a neutral position', () => {
  const { finding } = find('174030627562', 26);
  assert.deepEqual(positionAtPly(26, [finding]), {
    ply: 26,
    mode: 'played',
    findingPly: 26,
  });
  assert.deepEqual(positionAtPly(25, [finding]), {
    ply: 25,
    mode: 'before',
    findingPly: 26,
  });
  assert.deepEqual(positionAtPly(20, [finding]), { ply: 20, mode: 'game' });
  assert.deepEqual(findingPosition(finding, 'before'), {
    ply: 25,
    mode: 'before',
    findingPly: 26,
  });
  assert.deepEqual(findingPosition(finding, 'better'), {
    ply: 25,
    mode: 'better',
    findingPly: 26,
    linePly: 1,
  });
});

test('opponent continuations start after the played move and old saved reviews remain compatible', () => {
  const { raw, finding } = find('174030627562', 26);
  assert.deepEqual(finding.refutationPv, []);
  const board = new Chess(raw.after_fen);
  const reply = board.moves({ verbose: true })[0];
  const converted = convertFinding({ ...raw, refutation_variation_uci: [reply.lan] });
  assert.deepEqual(converted.refutationPv, [reply.san]);
  assert.equal(
    variationFrames(converted.afterFen, converted.refutationPv!)[1].fen,
    reply.after,
  );
  assert.deepEqual(findingPosition(converted, 'refutation'), {
    ply: 26,
    mode: 'refutation',
    findingPly: 26,
    linePly: 1,
  });
});

test('the accessible board describes pieces and whose turn it is without raw FEN', () => {
  const description = boardDescription(
    '8/1pqbk1p1/p1p1p2p/P1PpP1P1/1P1P3P/5QP1/6B1/6K1 w - - 2 31',
  );
  assert.match(description, /^White to move\./);
  assert.match(description, /queen on f3/);
  assert.match(description, /king on e7/);
  assert.doesNotMatch(description, /FEN|1pqbk1p1/);
});

test('shared links cannot select an explanation for a different original-game position', () => {
  const { finding } = find('174030627562', 26);
  for (const mode of ['before', 'played', 'better'] as const) {
    const restored = normaliseStudyPosition(
      { ply: 20, mode, findingPly: 26, linePly: 0 },
      [finding],
      100,
    );
    assert.equal(restored.ply, mode === 'played' ? 26 : 25);
    assert.equal(restored.findingPly, 26);
    assert.equal(restored.mode, mode);
  }
  assert.deepEqual(
    normaliseStudyPosition({ ply: 20, mode: 'played', findingPly: 99 }, [finding], 100),
    { ply: 20, mode: 'game' },
  );
  assert.deepEqual(
    normaliseStudyPosition({ ply: 999, mode: 'before', findingPly: 99 }, [finding], 100),
    { ply: 100, mode: 'game' },
  );
});

test('unavailable old refutations fall back to the played board and long lines clamp safely', () => {
  const { finding } = find('173984600836', 72);
  assert.deepEqual(
    normaliseStudyPosition(
      { ply: 0, mode: 'refutation', findingPly: 72, linePly: 50 },
      [finding],
      100,
    ),
    { ply: 72, mode: 'played', findingPly: 72 },
  );
  const restored = normaliseStudyPosition(
    { ply: 0, mode: 'better', findingPly: 72, linePly: 999 },
    [finding],
    100,
  );
  assert.equal(restored.ply, 71);
  assert.equal(restored.linePly, 15);
  assert.equal(
    normaliseStudyPosition(
      { ply: 71, mode: 'better', findingPly: 72, linePly: 0 },
      [finding],
      100,
    ).linePly,
    0,
  );
});
