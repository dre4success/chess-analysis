import { Chess } from 'chess.js';
import type { Eval, Game, GameAnalysis } from './chess.ts';
export const SCAN_NODES = 150000,
  CONFIRM_NODES = 1000000;
export type SearchResult = { score: Eval; pv: string[]; finalBest: string; info: string };
export function exactInfo(line: string): Omit<SearchResult, 'finalBest' | 'info'> | null {
  if (
    !line.startsWith('info ') ||
    line.startsWith('info string ') ||
    /\b(lowerbound|upperbound)\b/.test(line) ||
    (/\bmultipv (\d+)/.test(line) && !line.includes('multipv 1 '))
  )
    return null;
  const match = line.match(/\bscore (cp|mate) (-?\d+)\b/),
    pv = line.match(/\bpv (.+)$/);
  if (!match || !pv) return null;
  const moves = pv[1].trim().split(/\s+/);
  if (!moves.every((m) => /^[a-h][1-8][a-h][1-8][qrbn]?$/.test(m))) return null;
  return {
    score: { type: match[1] as Eval['type'], value: Number(match[2]) },
    pv: moves,
  };
}
export interface PositionEngine {
  search(fen: string, nodes: number): Promise<SearchResult>;
  close(): void;
}
export class BrowserEngine implements PositionEngine {
  private worker: Worker;
  private handler: ((line: string) => void) | null = null;
  private fail: ((error: Error) => void) | null = null;
  private closed = false;
  private signal: AbortSignal;
  constructor(signal: AbortSignal) {
    this.signal = signal;
    this.worker = new Worker('/engine/stockfish-18-lite-single.js');
    this.worker.onmessage = (e) => {
      for (const line of String(e.data).split('\n')) this.handler?.(line);
    };
    this.worker.onerror = () => {
      this.fail?.(
        new Error(
          'The chess engine could not start. Your game overview is still available.',
        ),
      );
      this.close();
    };
    signal.addEventListener('abort', this.close, { once: true });
  }
  private command(message: string, check: (line: string) => boolean) {
    return new Promise<void>((resolve, reject) => {
      if (this.closed || this.signal.aborted) {
        reject(new DOMException('Cancelled', 'AbortError'));
        return;
      }
      const timer = setTimeout(() => {
        reject(new Error('The chess engine took too long. Retry analysis on this game.'));
        this.close();
      }, 45000);
      this.fail = (e) => {
        clearTimeout(timer);
        this.handler = null;
        this.fail = null;
        reject(e);
      };
      this.handler = (line) => {
        if (check(line)) {
          clearTimeout(timer);
          this.handler = null;
          this.fail = null;
          resolve();
        }
      };
      this.worker.postMessage(message);
    });
  }
  async ready() {
    await this.command('uci', (line) => line === 'uciok');
    this.worker.postMessage('setoption name Hash value 64');
    this.worker.postMessage('setoption name Threads value 1');
    this.worker.postMessage('setoption name MultiPV value 1');
    await this.command('isready', (line) => line === 'readyok');
  }
  async search(fen: string, nodes: number): Promise<SearchResult> {
    this.worker.postMessage('setoption name Clear Hash');
    await this.command('isready', (line) => line === 'readyok');
    this.worker.postMessage(`position fen ${fen}`);
    let exact: ReturnType<typeof exactInfo> = null;
    let finalBest = '',
      infoLine = '';
    await this.command(`go nodes ${nodes}`, (line) => {
      const info = exactInfo(line);
      if (info) {
        exact = info;
        infoLine = line;
      }
      if (line.startsWith('bestmove ')) {
        finalBest = line.split(' ')[1];
        return true;
      }
      return false;
    });
    const result = exact as ReturnType<typeof exactInfo>;
    if (!result)
      throw new Error(
        'The engine did not return a complete evaluation. Please retry this game.',
      );
    const position = new Chess(fen);
    const first = result.pv[0];
    position.move({
      from: first.slice(0, 2),
      to: first.slice(2, 4),
      promotion: first[4],
    });
    return { ...result, finalBest, info: infoLine };
  }
  close = () => {
    if (this.closed) return;
    this.closed = true;
    this.worker.terminate();
    this.fail?.(new DOMException('Cancelled', 'AbortError'));
    this.signal.removeEventListener('abort', this.close);
  };
}

import { RustReview, convertFinding } from './rust-core.ts';
export async function analyseGame(
  game: Game,
  engine: PositionEngine,
  onProgress: (done: number, total: number) => void,
  signal: AbortSignal,
): Promise<GameAnalysis> {
  const rust = await RustReview.create();
  let step = rust.call({
    type: 'start',
    game: game.raw,
    username: game.colour === 'w' ? game.raw.white.username : game.raw.black.username,
    now: Math.floor(Date.now() / 1000),
  });
  while (step.requests.length) {
    const responses = [];
    for (let i = 0; i < step.requests.length; i++) {
      if (signal.aborted) throw new DOMException('Cancelled', 'AbortError');
      const req = step.requests[i],
        result = await engine.search(req.fen, req.nodes);
      responses.push({
        fen: req.fen,
        nodes: req.nodes,
        info: result.info,
        final_best: result.finalBest,
      });
      onProgress(i + 1, step.requests.length);
    }
    step = rust.call({ type: 'continue', responses });
  }
  return {
    id: game.id,
    source: 'rust-browser',
    positions: step.positions,
    findings: step.findings.map(convertFinding),
  };
}
