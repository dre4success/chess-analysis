import { Chess } from 'chess.js';
import type { Eval, Finding } from './chess.ts';

export type StudyMode = 'game' | 'before' | 'played' | 'better' | 'refutation';
export type StudyPosition = {
  ply: number;
  mode: StudyMode;
  findingPly?: number;
  linePly?: number;
};
export type EvaluationChange =
  | { kind: 'pawns'; centipawns: number; label: string }
  | { kind: 'missed-mate' | 'allowed-mate' | 'mate-change'; label: string };

function losingMate(e: Eval) {
  return e.type === 'mate' && (e.value < 0 || Object.is(e.value, -0));
}
export function evaluationChange(before: Eval, after: Eval): EvaluationChange {
  if (before.type === 'cp' && after.type === 'cp') {
    const centipawns = before.value - after.value;
    return {
      kind: 'pawns',
      centipawns,
      label: `−${(centipawns / 100).toFixed(1)} pawns`,
    };
  }
  if (losingMate(after) && !losingMate(before))
    return { kind: 'allowed-mate', label: 'Allowed forced mate' };
  if (before.type === 'mate' && !losingMate(before) && after.type === 'cp')
    return { kind: 'missed-mate', label: 'Missed forced mate' };
  return { kind: 'mate-change', label: 'Forced-mate change' };
}

/** The explanation follows the actual board position, never an unrelated selected tab. */
export function positionAtPly(ply: number, findings: Finding[] = []): StudyPosition {
  const played = findings.find((f) => f.ply === ply);
  if (played) return { ply, mode: 'played', findingPly: played.ply };
  const before = findings.find((f) => f.ply === ply + 1);
  return before ? { ply, mode: 'before', findingPly: before.ply } : { ply, mode: 'game' };
}
export function findingPosition(
  finding: Finding,
  mode: Exclude<StudyMode, 'game'>,
): StudyPosition {
  return {
    ply: finding.ply - (mode === 'before' || mode === 'better' ? 1 : 0),
    mode,
    findingPly: finding.ply,
    ...(mode === 'better' || mode === 'refutation' ? { linePly: 1 } : {}),
  };
}
/** Resolve shared-link coordinates against the actual reviewed game. */
export function normaliseStudyPosition(
  position: StudyPosition,
  findings: Finding[] = [],
  gamePlies: number,
): StudyPosition {
  const ply = Number.isFinite(position.ply)
    ? Math.max(0, Math.min(gamePlies, Math.floor(position.ply)))
    : 0;
  const neutral: StudyPosition = { ply, mode: 'game' };
  if (position.mode === 'game') return neutral;
  const finding = findings.find(
    (f) => f.ply === position.findingPly && f.ply > 0 && f.ply <= gamePlies,
  );
  if (!finding) return neutral;
  const mode =
    position.mode === 'refutation' && !finding.refutationPv?.length
      ? 'played'
      : position.mode === 'better' && !finding.pv.length
        ? 'before'
        : position.mode;
  const result = findingPosition(finding, mode);
  if (mode === 'better' || mode === 'refutation') {
    const length = mode === 'better' ? finding.pv.length : finding.refutationPv!.length;
    result.linePly = Number.isFinite(position.linePly)
      ? Math.max(0, Math.min(length, Math.floor(position.linePly!)))
      : 1;
  }
  return result;
}
export function variationFrames(fen: string, moves: string[]) {
  const chess = new Chess(fen);
  const frames = [{ fen: chess.fen(), squares: [] as string[], san: '' }];
  for (const san of moves) {
    const move = chess.move(san);
    frames.push({ fen: chess.fen(), squares: [move.from, move.to], san: move.san });
  }
  return frames;
}

const names: Record<string, string> = {
  p: 'pawn',
  n: 'knight',
  b: 'bishop',
  r: 'rook',
  q: 'queen',
  k: 'king',
};
export function pieceName(piece: string) {
  return piece
    ? `${piece === piece.toUpperCase() ? 'White' : 'Black'} ${names[piece.toLowerCase()]}`
    : 'empty';
}
export function boardDescription(fen: string) {
  const board = new Chess(fen);
  const white: string[] = [],
    black: string[] = [];
  for (const rank of board.board())
    for (const piece of rank) {
      if (piece)
        (piece.color === 'w' ? white : black).push(
          `${names[piece.type]} on ${piece.square}`,
        );
    }
  return `${board.turn() === 'w' ? 'White' : 'Black'} to move. White: ${white.join(', ')}. Black: ${black.join(', ')}.`;
}

export function practiceHint(finding: Finding) {
  const piece = new Chess(finding.beforeFen).get(
    finding.bestUci.slice(0, 2) as import('chess.js').Square,
  );
  return piece
    ? `Consider your ${names[piece.type]} on ${finding.bestUci.slice(0, 2)}. Look at its checks, captures and threats.`
    : 'Look at checks, captures and threats before choosing your move.';
}
