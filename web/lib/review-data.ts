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
  clock_secs?: number;
};
export type SavedReview = {
  user: string;
  generated: string;
  games: { url: string; findings: RustFinding[] }[];
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
  const c = new Chess(f.before_fen),
    pv = [];
  for (const u of f.principal_variation_uci.slice(0, 10)) {
    const m = c.move({ from: u.slice(0, 2), to: u.slice(2, 4), promotion: u[4] });
    pv.push(m.san);
  }
  return {
    ply: f.ply,
    beforeFen: f.before_fen,
    afterFen: f.after_fen,
    actual: f.actual_san,
    best: f.best_san,
    bestUci: f.best_uci,
    before: convertEval(f.eval_before),
    after: convertEval(f.eval_after),
    loss:
      f.eval_before.type === 'cp' && f.eval_after.type === 'cp'
        ? f.eval_before.value - f.eval_after.value
        : null,
    title: titles[f.classification] ?? 'A moment to review',
    explanation:
      f.classification === 'engine-verified-mistake'
        ? `Stockfish prefers ${f.best_san} to ${f.actual_san}. Replay the alternative to see how the position changes.`
        : f.explanation,
    pv,
  };
}
