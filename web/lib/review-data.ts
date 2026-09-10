import { Chess } from 'chess.js';
import type { Eval, Finding } from './chess.ts';
export type RustEval =
  | { type: 'cp'; value: number; pov: 'user' }
  | { type: 'mate'; winner: 'user' | 'opponent'; moves: number; pov: 'user' };
export type RustFinding = {
  ply: number;
  before_fen: string;
  after_fen: string;
  actual_uci: string;
  actual_san: string;
  best_uci: string;
  best_san: string;
  eval_before: RustEval;
  eval_after: RustEval;
  classification: string;
  explanation: string;
  principal_variation_uci: string[];
  refutation_variation_uci?: string[];
  clock_secs?: number;
};
export type ReviewPattern = {
  classification: string;
  games_affected: number;
  games_reviewed: number;
  occurrences: number;
  example_refs: { url: string; ply: number }[];
  breakdowns: {
    dimension: string;
    value: string;
    games_affected: number;
    games_reviewed: number;
    occurrences: number;
  }[];
};
export type SavedReview = {
  user: string;
  generated: string;
  games: {
    url: string;
    findings: RustFinding[];
    clock_used_pct?: number;
    phase_coverage?: string[];
    clock_band_coverage?: string[];
  }[];
  patterns?: ReviewPattern[];
};
function convertEval(e: RustEval): Eval {
  return e.type === 'cp'
    ? { type: 'cp', value: e.value }
    : { type: 'mate', value: (e.winner === 'user' ? 1 : -1) * e.moves };
}
const titles: Record<string, string> = {
  'missed-mate': 'A mating opportunity',
  'line-opened': 'A line opened to your piece',
  'line-onto': 'A piece moved into the line of fire',
  'defender-left': 'A defender moved away',
  'attacked-piece-ignored': 'An attacked piece needed attention',
  'capture-cost': 'A capture cost more than it gained',
  'engine-verified-mistake': 'A stronger move was available',
};
export function convertFinding(f: RustFinding): Finding {
  function notation(fen: string, moves: string[]) {
    const c = new Chess(fen),
      pv = [];
    for (const u of moves) {
      const m = c.move({ from: u.slice(0, 2), to: u.slice(2, 4), promotion: u[4] });
      pv.push(m.san);
    }
    return pv;
  }
  const missedMate =
    f.eval_before.type === 'mate' &&
    f.eval_before.winner === 'user' &&
    f.eval_after.type === 'cp';
  return {
    ply: f.ply,
    beforeFen: f.before_fen,
    afterFen: f.after_fen,
    actual: f.actual_san,
    best: f.best_san,
    bestUci: f.best_uci,
    classification: f.classification,
    clockSecs: f.clock_secs,
    before: convertEval(f.eval_before),
    after: convertEval(f.eval_after),
    loss:
      f.eval_before.type === 'cp' && f.eval_after.type === 'cp'
        ? f.eval_before.value - f.eval_after.value
        : null,
    title: missedMate
      ? 'A mating opportunity'
      : (titles[f.classification] ?? 'A moment to review'),
    explanation:
      f.classification === 'engine-verified-mistake'
        ? missedMate
          ? `${f.best_san} keeps a forced mate. After ${f.actual_san}, that mating opportunity is gone. Follow the better line to see the forcing moves.`
          : `Stockfish prefers ${f.best_san} to ${f.actual_san}. Replay the alternative to see how the position changes.`
        : f.explanation,
    pv: notation(f.before_fen, f.principal_variation_uci),
    refutationPv: notation(f.after_fen, f.refutation_variation_uci ?? []),
  };
}
