'use client';
import { Fragment, useMemo, useState } from 'react';
import { Chess } from 'chess.js';
import {
  ArrowUpRight,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  FlipVertical2,
  LoaderCircle,
  ScanSearch,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  dateLabel,
  evalLabel,
  timeLabel,
  type Finding,
  type Game,
  type GameAnalysis,
} from '@/lib/chess';
export function Board({
  fen,
  flipped = false,
  highlights = [],
}: {
  fen: string;
  flipped?: boolean;
  highlights?: string[];
}) {
  const pieces = fen
    .split(' ')[0]
    .split('/')
    .flatMap((rank, r) =>
      rank
        .split('')
        .flatMap((x) => (/\d/.test(x) ? Array(Number(x)).fill('') : x))
        .map((piece, c) => ({ piece, square: `${'abcdefgh'[c]}${8 - r}`, r, c })),
    );
  if (flipped) pieces.reverse();
  return (
    <div
      className="chess-board"
      role="img"
      aria-label={`Chess position. ${flipped ? 'Black' : 'White'} at the bottom. FEN: ${fen}`}
    >
      {pieces.map(({ piece, square, r, c }, i) => (
        <div
          key={square}
          className={`square ${(r + c) % 2 ? 'dark-square' : 'light-square'} ${highlights.includes(square) ? 'highlight' : ''}`}
        >
          <span className={piece === piece.toUpperCase() ? 'white-piece' : 'black-piece'}>
            {
              (
                { k: '♚', q: '♛', r: '♜', b: '♝', n: '♞', p: '♟' } as Record<
                  string,
                  string
                >
              )[piece.toLowerCase()]
            }
          </span>
          {i % 8 === 0 && <small className="rank-label">{8 - r}</small>}
          {i >= 56 && <small className="file-label">{'abcdefgh'[c]}</small>}
        </div>
      ))}
    </div>
  );
}
export default function GameReview({
  game,
  games,
  analyses,
  onSelect,
  analysis,
  onAnalyse,
  busy,
  progress,
  focusPly,
}: {
  game: Game;
  games: Game[];
  analyses: Record<string, GameAnalysis>;
  onSelect: (id: string) => void;
  analysis?: GameAnalysis;
  onAnalyse: () => void;
  busy: boolean;
  progress: string;
  focusPly?: number;
}) {
  const initialIndex = Math.max(
    0,
    analysis?.findings.findIndex((f) => f.ply === focusPly) ?? 0,
  );
  const [ply, setPly] = useState(() =>
      analysis?.findings[initialIndex] ? analysis.findings[initialIndex].ply - 1 : 0,
    ),
    [flipped, setFlipped] = useState(game.colour === 'b'),
    [findingIndex, setFindingIndex] = useState(initialIndex),
    [line, setLine] = useState(false),
    [linePly, setLinePly] = useState(0);
  const finding = analysis?.findings[findingIndex];
  const lineFrames = useMemo(() => {
    if (!finding) return [];
    const c = new Chess(finding.beforeFen),
      frames = [{ fen: c.fen(), squares: [] as string[] }];
    for (const san of finding.pv) {
      try {
        const m = c.move(san);
        frames.push({ fen: c.fen(), squares: [m.from, m.to] });
      } catch {
        break;
      }
    }
    return frames;
  }, [finding]);
  const last = game.moves[ply - 1],
    fen =
      line && lineFrames[linePly]
        ? lineFrames[linePly].fen
        : ply === 0
          ? game.moves[0].before
          : last.after;
  const active = line ? linePly : ply,
    max = line ? lineFrames.length - 1 : game.moves.length;
  function jump(n: number) {
    if (line) setLinePly(Math.max(0, Math.min(max, n)));
    else setPly(Math.max(0, Math.min(max, n)));
  }
  function inspect(f: Finding, index: number) {
    setFindingIndex(index);
    setPly(f.ply - 1);
    setLine(false);
    setLinePly(0);
  }
  const topColour = flipped ? 'w' : 'b';
  // Moves are drawn as scoresheet rows (number, White, Black) rather than a flat
  // run of plies, so the numbering carries its normal meaning.
  const rows = Array.from({ length: Math.ceil(game.moves.length / 2) }, (_, i) => ({
    number: i + 1,
    white: { move: game.moves[i * 2], ply: i * 2 + 1 },
    black: game.moves[i * 2 + 1] ? { move: game.moves[i * 2 + 1], ply: i * 2 + 2 } : null,
  }));
  const drop =
    finding &&
    (finding.loss === null ? 'to mate' : `−${(finding.loss / 100).toFixed(1)}`);
  return (
    <section className="review-layout">
      <div className="game-rail" aria-label="Your games">
        <div className="rail-heading">
          <strong>Your games</strong>
          <span>
            {games.filter((g) => analyses[g.id]).length}/{games.length} checked
          </span>
        </div>
        {games.map((g) => {
          const found = analyses[g.id]?.findings.length;
          return (
            <button
              key={g.id}
              className="rail-game"
              aria-current={g.id === game.id}
              onClick={() => onSelect(g.id)}
            >
              <span
                className={`colour-dot ${g.colour === 'w' ? 'white-dot' : 'black-dot'}`}
                aria-hidden="true"
              />
              <span>
                <strong>{g.opponent}</strong>
                <small>
                  {g.result === 'win' ? 'Won' : g.result === 'loss' ? 'Lost' : 'Drew'}
                  {' · '}
                  {dateLabel(g.date)}
                </small>
              </span>
              {analyses[g.id] ? (
                <span className="rail-moments">{found === 0 ? '—' : `${found} key`}</span>
              ) : (
                <span className="rail-pending">Not checked</span>
              )}
            </button>
          );
        })}
      </div>
      <div className="review-board-panel">
        <div className="board-person">
          <span className={`mini-avatar ${topColour === game.colour ? 'peach' : ''}`}>
            {topColour === game.colour ? 'You' : game.opponent[0].toUpperCase()}
          </span>
          <div>
            <strong>{topColour === game.colour ? 'You' : game.opponent}</strong>
            <span>
              {topColour === 'w' ? 'White' : 'Black'} ·{' '}
              {topColour === game.colour ? game.rating : game.opponentRating}
            </span>
          </div>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Flip board"
            onClick={() => setFlipped(!flipped)}
          >
            <FlipVertical2 size={17} />
          </Button>
        </div>
        <div
          role="slider"
          aria-valuemin={0}
          aria-valuemax={max}
          aria-valuenow={active}
          aria-valuetext={`${line ? 'Engine line' : 'Game'}: move ${active} of ${max}`}
          tabIndex={0}
          className="board-keyboard"
          aria-label="Use left and right arrow keys to replay the game"
          onKeyDown={(e) => {
            if (
              ['ArrowLeft', 'ArrowDown', 'ArrowRight', 'ArrowUp', 'Home', 'End'].includes(
                e.key,
              )
            ) {
              e.preventDefault();
              jump(
                e.key === 'Home'
                  ? 0
                  : e.key === 'End'
                    ? max
                    : active + (['ArrowLeft', 'ArrowDown'].includes(e.key) ? -1 : 1),
              );
            }
          }}
        >
          <Board
            fen={fen}
            flipped={flipped}
            highlights={
              line ? lineFrames[linePly]?.squares : last ? [last.from, last.to] : []
            }
          />
        </div>
        <div className="replay-controls">
          <Button
            variant="ghost"
            size="icon"
            aria-label="First position"
            onClick={() => jump(0)}
            disabled={active === 0}
          >
            <ChevronsLeft />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Previous move"
            onClick={() => jump(active - 1)}
            disabled={active === 0}
          >
            <ChevronLeft />
          </Button>
          <span aria-live="polite">
            {line
              ? `Engine line ${active}/${max}`
              : ply === 0
                ? 'Starting position'
                : `${Math.ceil(ply / 2)}${ply % 2 ? '.' : '…'} ${last.san}`}
          </span>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Next move"
            onClick={() => jump(active + 1)}
            disabled={active >= max}
          >
            <ChevronRight />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Last position"
            onClick={() => jump(max)}
            disabled={active >= max}
          >
            <ChevronsRight />
          </Button>
        </div>
        <div className="board-person">
          <span className={`mini-avatar ${topColour !== game.colour ? 'peach' : ''}`}>
            {topColour !== game.colour ? 'You' : game.opponent[0].toUpperCase()}
          </span>
          <div>
            <strong>{topColour !== game.colour ? 'You' : game.opponent}</strong>
            <span>
              {topColour === 'w' ? 'Black' : 'White'} ·{' '}
              {topColour !== game.colour ? game.rating : game.opponentRating}
            </span>
          </div>
          <span className={`result-badge ${game.result}`}>
            {game.result === 'win' ? 'Won' : game.result === 'loss' ? 'Lost' : 'Draw'}
          </span>
        </div>
        <p className="board-note">
          {timeLabel(game.timeControl)} · {dateLabel(game.date)} ·{' '}
          {Math.ceil(game.moves.length / 2)} moves
        </p>
      </div>
      <div className="review-detail">
        <div>
          <div className="section-heading">
            <h2>You vs {game.opponent}</h2>
            <a
              href={game.id}
              target="_blank"
              rel="noreferrer"
              className="icon-link"
              aria-label="Open this game on Chess.com"
            >
              <ArrowUpRight size={19} />
            </a>
          </div>
          <p className="muted opening-name">{game.opening}</p>
        </div>
        <div>
          <div className="findings-header">
            <h3>Key moments</h3>
            {analysis && (
              <span className="subtle-chip">
                <Check size={13} />{' '}
                {analysis.source === 'native' ? 'Native Stockfish' : 'Verified review'}
              </span>
            )}
          </div>
          {!analysis ? (
            <div className="analysis-prompt">
              <ScanSearch size={24} />
              <h3>{busy ? 'Looking for turning points…' : 'Go beyond the result.'}</h3>
              <p>
                {busy
                  ? progress
                  : 'Find up to three moments where a different move could have made a difference.'}
              </p>
              <Button className="primary-button" onClick={onAnalyse} disabled={busy}>
                {busy ? <LoaderCircle className="spin" /> : <ScanSearch />}
                {busy ? 'Analysing this batch…' : 'Analyse this game'}
              </Button>
            </div>
          ) : analysis.findings.length === 0 ? (
            <div className="analysis-prompt">
              <Check size={24} />
              <h3>No large swings confirmed.</h3>
              <p>
                This review did not confirm a two-pawn loss on your moves. This is a
                limited search, rather than a perfect-play score.
              </p>
            </div>
          ) : (
            <>
              <div className="moment-tabs">
                {analysis.findings.map((f, i) => (
                  <Button
                    key={f.ply}
                    variant="ghost"
                    className={findingIndex === i ? 'active' : ''}
                    onClick={() => inspect(f, i)}
                  >
                    {Math.ceil(f.ply / 2)}
                    {game.colour === 'w' ? '.' : '…'} {f.actual}
                    <span>
                      {f.loss === null ? 'Mate' : `−${(f.loss / 100).toFixed(1)}`}
                    </span>
                  </Button>
                ))}
              </div>
              {finding && (
                <div className="finding-card">
                  <h4>{finding.title}</h4>
                  <p>{finding.explanation}</p>
                  <div className="eval-comparison">
                    <div>
                      <span>Before your move</span>
                      <strong>{evalLabel(finding.before)}</strong>
                    </div>
                    <span className="eval-drop">{drop}</span>
                    <div>
                      <span>After {finding.actual}</span>
                      <strong>{evalLabel(finding.after)}</strong>
                    </div>
                  </div>
                  <div className="line-actions">
                    <Button
                      variant="outline"
                      className={!line ? 'selected' : ''}
                      onClick={() => {
                        setLine(false);
                        setPly(finding.ply);
                      }}
                    >
                      Played {finding.actual}
                    </Button>
                    <Button
                      variant="outline"
                      className={line ? 'selected' : ''}
                      onClick={() => {
                        setLine(true);
                        setLinePly(1);
                      }}
                    >
                      Try {finding.best}
                    </Button>
                  </div>
                  {line && <p className="variation-line">{finding.pv.join('  ')}</p>}
                  <p className="evaluation-note">
                    Evaluation is from your side. Use the arrows to explore.
                  </p>
                </div>
              )}
            </>
          )}
        </div>
        <div>
          <div className="move-list-heading">
            <h3>Move by move</h3>
            <span>{rows.length} moves</span>
          </div>
          <div className="move-list" aria-label="Game moves">
            {rows.map((row) => (
              <Fragment key={row.number}>
                <span className="move-number">{row.number}.</span>
                {[row.white, row.black].map((entry, side) =>
                  entry ? (
                    <Button
                      key={side}
                      variant="ghost"
                      className={!line && ply === entry.ply ? 'current-move' : ''}
                      onClick={() => {
                        setLine(false);
                        setPly(entry.ply);
                      }}
                      aria-label={`Go to move ${row.number}, ${side === 0 ? 'White' : 'Black'}, ${entry.move.san}`}
                    >
                      {entry.move.san}
                      {analysis?.findings.some((f) => f.ply === entry.ply) && (
                        <i className="move-dot" />
                      )}
                    </Button>
                  ) : (
                    <span key={side} />
                  ),
                )}
              </Fragment>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
