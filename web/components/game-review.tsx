'use client';
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { Chess, type Square } from 'chess.js';
import {
  ArrowUpRight,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  FlipVertical2,
  Lightbulb,
  LoaderCircle,
  ScanSearch,
  Target,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import GameNavigator from './game-navigator';
import {
  dateLabel,
  evalLabel,
  timeLabel,
  type Finding,
  type Game,
  type GameAnalysis,
} from '@/lib/chess';
import {
  boardDescription,
  evaluationChange,
  findingPosition,
  normaliseStudyPosition,
  pieceName,
  positionAtPly,
  practiceHint,
  variationFrames,
  type StudyPosition,
} from '@/lib/study';

export function Board({
  fen,
  flipped = false,
  highlights = [],
  onSquareSelect,
  selectedSquare,
  legalSquares = [],
}: {
  fen: string;
  flipped?: boolean;
  highlights?: string[];
  onSquareSelect?: (square: string) => void;
  selectedSquare?: string;
  legalSquares?: string[];
}) {
  const [focused, setFocused] = useState('e4');
  const buttons = useRef<Record<string, HTMLButtonElement | null>>({});
  const interactive = !!onSquareSelect;
  useEffect(() => {
    if (interactive) buttons.current[focused]?.focus({ preventScroll: true });
  }, [interactive, focused]);
  const pieces = fen
    .split(' ')[0]
    .split('/')
    .flatMap((rank, r) =>
      rank
        .split('')
        .flatMap((x) => (/\d/.test(x) ? Array(Number(x)).fill('') : x))
        .map((piece: string, c) => ({ piece, square: `${'abcdefgh'[c]}${8 - r}`, r, c })),
    );
  if (flipped) pieces.reverse();
  const description = `${flipped ? 'Black' : 'White'} at the bottom. ${boardDescription(fen)}`;
  return (
    <div
      className={`chess-board ${onSquareSelect ? 'practice-board' : ''}`}
      role={onSquareSelect ? 'group' : 'img'}
      aria-label={
        onSquareSelect
          ? `Choose a piece, then its destination. Use arrow keys between squares. ${description}`
          : description
      }
    >
      {pieces.map(({ piece, square, r, c }, i) => {
        const className = `square ${(r + c) % 2 ? 'dark-square' : 'light-square'} ${highlights.includes(square) ? 'highlight' : ''} ${selectedSquare === square ? 'selected-square' : ''} ${legalSquares.includes(square) ? 'legal-square' : ''}`;
        const content = (
          <>
            <span
              className={
                piece && piece === piece.toUpperCase() ? 'white-piece' : 'black-piece'
              }
              aria-hidden="true"
            >
              {
                (
                  { k: '♚', q: '♛', r: '♜', b: '♝', n: '♞', p: '♟' } as Record<
                    string,
                    string
                  >
                )[piece.toLowerCase()]
              }
            </span>
            {i % 8 === 0 && (
              <small className="rank-label" aria-hidden="true">
                {8 - r}
              </small>
            )}
            {i >= 56 && (
              <small className="file-label" aria-hidden="true">
                {'abcdefgh'[c]}
              </small>
            )}
          </>
        );
        return onSquareSelect ? (
          <button
            type="button"
            key={square}
            ref={(node) => {
              buttons.current[square] = node;
            }}
            className={className}
            tabIndex={focused === square ? 0 : -1}
            aria-label={`${square}, ${pieceName(piece)}${legalSquares.includes(square) ? ', legal destination' : ''}`}
            aria-pressed={selectedSquare === square}
            onFocus={() => setFocused(square)}
            onClick={() => onSquareSelect(square)}
            onKeyDown={(event) => {
              const delta: Record<string, number> = {
                ArrowLeft: -1,
                ArrowRight: 1,
                ArrowUp: -8,
                ArrowDown: 8,
              };
              if (!(event.key in delta)) return;
              event.preventDefault();
              const nextIndex = i + delta[event.key];
              if (
                nextIndex < 0 ||
                nextIndex > 63 ||
                ((event.key === 'ArrowLeft' || event.key === 'ArrowRight') &&
                  Math.floor(i / 8) !== Math.floor(nextIndex / 8))
              )
                return;
              buttons.current[pieces[nextIndex].square]?.focus();
            }}
          >
            {content}
          </button>
        ) : (
          <div key={square} className={className}>
            {content}
          </div>
        );
      })}
    </div>
  );
}

type Practice = {
  findingPly: number;
  selected?: string;
  attemptFen?: string;
  attempt?: string;
  correct?: boolean;
  hint?: boolean;
  promotion?: { from: string; to: string };
};
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
  position,
  onPositionChange,
  samplePlayer,
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
  position?: StudyPosition;
  onPositionChange?: (position: StudyPosition) => void;
  samplePlayer?: string;
}) {
  const [localPosition, setLocalPosition] = useState<StudyPosition>(() => {
    const first =
      analysis?.findings.find((f) => f.ply === focusPly) ?? analysis?.findings[0];
    return first ? findingPosition(first, 'before') : { ply: 0, mode: 'game' };
  });
  const [flipped, setFlipped] = useState(game.colour === 'b');
  const [practice, setPractice] = useState<Practice | null>(null);
  const moveList = useRef<HTMLDivElement>(null);
  const currentMove = useRef<HTMLButtonElement>(null);
  const current = normaliseStudyPosition(
    position ?? localPosition,
    analysis?.findings,
    game.moves.length,
  );
  const ply = Math.min(game.moves.length, Math.max(0, current.ply));
  const finding =
    current.mode === 'game'
      ? undefined
      : analysis?.findings.find((f) => f.ply === current.findingPly);
  const inLine =
    !!finding && (current.mode === 'better' || current.mode === 'refutation');
  const frames = useMemo(() => {
    if (!finding) return [];
    return variationFrames(
      current.mode === 'refutation' ? finding.afterFen : finding.beforeFen,
      current.mode === 'refutation' ? (finding.refutationPv ?? []) : finding.pv,
    );
  }, [finding, current.mode]);
  const linePly = Math.max(0, Math.min(frames.length - 1, current.linePly ?? 0));
  const last = game.moves[ply - 1];
  const practicingFinding =
    practice && analysis?.findings.find((f) => f.ply === practice.findingPly);
  const practicing =
    !!practice &&
    !!practicingFinding &&
    current.mode === 'before' &&
    practice.findingPly === finding?.ply;
  const fen = practicing
    ? (practice.attemptFen ?? practicingFinding.beforeFen)
    : inLine
      ? frames[linePly].fen
      : ply === 0
        ? game.moves[0].before
        : last.after;
  const active = inLine ? linePly : ply;
  const max = inLine ? frames.length - 1 : game.moves.length;
  const legalSquares = useMemo(() => {
    if (!practicing || !practice.selected || practice.attemptFen) return [];
    return new Chess(practicingFinding.beforeFen)
      .moves({ square: practice.selected as Square, verbose: true })
      .map((move) => move.to);
  }, [practice, practicing, practicingFinding]);

  function navigate(next: StudyPosition) {
    setPractice(null);
    setLocalPosition(next);
    onPositionChange?.(next);
  }
  function jump(n: number) {
    const target = Math.max(0, Math.min(max, n));
    navigate(
      inLine
        ? { ...current, linePly: target }
        : positionAtPly(target, analysis?.findings),
    );
  }
  function inspect(f: Finding) {
    navigate(findingPosition(f, 'before'));
  }
  function startPractice() {
    if (!finding) return;
    navigate(findingPosition(finding, 'before'));
    setPractice({ findingPly: finding.ply });
  }
  function tryMove(from: string, to: string, promotion?: string) {
    if (!practicingFinding || !practice) return;
    const board = new Chess(practicingFinding.beforeFen);
    try {
      const move = board.move({ from, to, promotion });
      setPractice({
        ...practice,
        selected: undefined,
        promotion: undefined,
        attemptFen: board.fen(),
        attempt: move.san,
        correct: move.lan === practicingFinding.bestUci,
      });
    } catch {
      setPractice({ ...practice, selected: undefined, promotion: undefined });
    }
  }
  function selectSquare(square: string) {
    if (!practice || !practicingFinding || practice.attemptFen) return;
    const board = new Chess(practicingFinding.beforeFen);
    const piece = board.get(square as Square);
    if (piece?.color === board.turn()) {
      setPractice({
        ...practice,
        selected: square === practice.selected ? undefined : square,
        promotion: undefined,
      });
    } else if (practice.selected && legalSquares.includes(square as Square)) {
      const options = board
        .moves({ square: practice.selected as Square, verbose: true })
        .filter((move) => move.to === square);
      if (options.some((move) => move.promotion)) {
        setPractice({ ...practice, promotion: { from: practice.selected, to: square } });
      } else tryMove(practice.selected, square);
    }
  }
  useEffect(() => {
    const list = moveList.current,
      selected = currentMove.current;
    if (!list || !selected) return;
    const bounds = list.getBoundingClientRect(),
      move = selected.getBoundingClientRect();
    if (move.top < bounds.top) list.scrollTop -= bounds.top - move.top;
    else if (move.bottom > bounds.bottom) list.scrollTop += move.bottom - bounds.bottom;
  }, [ply, inLine]);

  const topColour = flipped ? 'w' : 'b';
  const rows = Array.from({ length: Math.ceil(game.moves.length / 2) }, (_, i) => ({
    number: i + 1,
    white: { move: game.moves[i * 2], ply: i * 2 + 1 },
    black: game.moves[i * 2 + 1] ? { move: game.moves[i * 2 + 1], ply: i * 2 + 2 } : null,
  }));
  const positionLabel = practicing
    ? 'Your move · choose a piece and a destination'
    : inLine
      ? `${current.mode === 'refutation' ? 'Opponent’s reply' : 'Better line'} · ${linePly}/${max}${frames[linePly].san ? ` · ${frames[linePly].san}` : ''}`
      : ply === 0
        ? 'Starting position'
        : `${Math.ceil(ply / 2)}${ply % 2 ? '.' : '…'} ${last.san}`;
  const replayDescription = inLine
    ? positionLabel
    : ply === 0
      ? 'Starting position'
      : `After move ${Math.ceil(ply / 2)}, ${ply % 2 ? 'White' : 'Black'}, ${last.san}`;
  const board = (
    <Board
      fen={fen}
      flipped={flipped}
      highlights={
        inLine
          ? frames[linePly]?.squares
          : last && !practicing
            ? [last.from, last.to]
            : []
      }
      onSquareSelect={practicing && !practice.attemptFen ? selectSquare : undefined}
      selectedSquare={practicing ? practice.selected : undefined}
      legalSquares={legalSquares}
    />
  );

  return (
    <section className="review-layout" aria-label="Game study">
      <GameNavigator
        games={games}
        analyses={analyses}
        selectedId={game.id}
        onSelect={onSelect}
        sample={!!samplePlayer}
      />
      <div className="review-board-panel">
        <div className="board-person">
          <span className={`mini-avatar ${topColour === game.colour ? 'peach' : ''}`}>
            {topColour === game.colour
              ? (samplePlayer?.[0] ?? 'You')
              : game.opponent[0].toUpperCase()}
          </span>
          <div>
            <strong>
              {topColour === game.colour ? (samplePlayer ?? 'You') : game.opponent}
            </strong>
            <span>
              {topColour === 'w' ? 'White' : 'Black'} ·{' '}
              {topColour === game.colour ? game.rating : game.opponentRating}
            </span>
          </div>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Flip board"
            aria-pressed={flipped}
            onClick={() => setFlipped(!flipped)}
          >
            <FlipVertical2 size={17} />
          </Button>
        </div>
        {practicing ? (
          board
        ) : (
          <div
            role="slider"
            aria-valuemin={0}
            aria-valuemax={max}
            aria-valuenow={active}
            aria-valuetext={replayDescription}
            tabIndex={0}
            className="board-keyboard"
            aria-label="Replay position. Use left and right arrow keys."
            onKeyDown={(event) => {
              if (
                practicing ||
                ![
                  'ArrowLeft',
                  'ArrowDown',
                  'ArrowRight',
                  'ArrowUp',
                  'Home',
                  'End',
                ].includes(event.key)
              )
                return;
              event.preventDefault();
              jump(
                event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? max
                    : active + (['ArrowLeft', 'ArrowDown'].includes(event.key) ? -1 : 1),
              );
            }}
          >
            {board}
          </div>
        )}
        <div className="replay-controls">
          <Button
            variant="ghost"
            size="icon"
            aria-label="First position"
            onClick={() => jump(0)}
            disabled={active === 0 || practicing}
          >
            <ChevronsLeft />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Previous move"
            onClick={() => jump(active - 1)}
            disabled={active === 0 || practicing}
          >
            <ChevronLeft />
          </Button>
          <span aria-live="polite">{positionLabel}</span>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Next move"
            onClick={() => jump(active + 1)}
            disabled={active >= max || practicing}
          >
            <ChevronRight />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Last position"
            onClick={() => jump(max)}
            disabled={active >= max || practicing}
          >
            <ChevronsRight />
          </Button>
        </div>
        {finding && !practicing && (
          <div className="board-comparison">
            <div
              className="line-actions"
              role="group"
              aria-label="Compare this key moment"
            >
              <Button
                variant="outline"
                className={current.mode === 'before' ? 'selected' : ''}
                aria-pressed={current.mode === 'before'}
                onClick={() => navigate(findingPosition(finding, 'before'))}
              >
                Before
              </Button>
              <Button
                variant="outline"
                className={current.mode === 'played' ? 'selected' : ''}
                aria-pressed={current.mode === 'played'}
                onClick={() => navigate(findingPosition(finding, 'played'))}
              >
                Played {finding.actual}
              </Button>
              <Button
                variant="outline"
                className={current.mode === 'better' ? 'selected' : ''}
                aria-pressed={current.mode === 'better'}
                onClick={() => navigate(findingPosition(finding, 'better'))}
              >
                Better {finding.best}
              </Button>
            </div>
            <div className="study-actions">
              {!!finding.refutationPv?.length && (
                <Button
                  variant="ghost"
                  aria-pressed={current.mode === 'refutation'}
                  className={current.mode === 'refutation' ? 'selected' : ''}
                  onClick={() => navigate(findingPosition(finding, 'refutation'))}
                >
                  See the opponent’s reply
                </Button>
              )}
              <Button variant="ghost" onClick={startPractice}>
                <Target size={15} /> Try it yourself
              </Button>
            </div>
          </div>
        )}
        {practicing && (
          <div className="practice-panel" aria-label="Find a move exercise">
            <div className="practice-heading">
              <Target size={17} />
              <strong>
                {practice.attempt
                  ? `Your candidate: ${practice.attempt}`
                  : 'What would you play?'}
              </strong>
            </div>
            <p role="status">
              {practice.attempt
                ? practice.correct
                  ? 'You found the move that starts the verified line. Now explore why it works.'
                  : 'A legal candidate. The verified line starts with a different move; compare them when you’re ready.'
                : 'Choose one of your pieces, then a destination. Look for checks, captures and threats.'}
            </p>
            {practice.hint && (
              <p className="practice-hint">
                <Lightbulb size={15} />
                {practiceHint(practicingFinding)}
              </p>
            )}
            {practice.promotion && (
              <div
                className="promotion-choices"
                role="group"
                aria-label="Choose your promotion"
              >
                {(['q', 'r', 'b', 'n'] as const).map((piece) => (
                  <Button
                    key={piece}
                    variant="outline"
                    onClick={() =>
                      tryMove(practice.promotion!.from, practice.promotion!.to, piece)
                    }
                  >
                    {{ q: 'Queen', r: 'Rook', b: 'Bishop', n: 'Knight' }[piece]}
                  </Button>
                ))}
              </div>
            )}
            <div className="practice-actions">
              {practice.attempt ? (
                <Button
                  variant="outline"
                  onClick={() => setPractice({ findingPly: practicingFinding.ply })}
                >
                  Try another move
                </Button>
              ) : (
                <Button
                  variant="outline"
                  onClick={() => setPractice({ ...practice, hint: true })}
                  disabled={practice.hint}
                >
                  <Lightbulb size={15} />
                  Hint
                </Button>
              )}
              <Button
                className="primary-button"
                onClick={() => navigate(findingPosition(practicingFinding, 'better'))}
              >
                Reveal the line <ArrowUpRight size={15} />
              </Button>
            </div>
          </div>
        )}
        <div className="board-person">
          <span className={`mini-avatar ${topColour !== game.colour ? 'peach' : ''}`}>
            {topColour !== game.colour
              ? (samplePlayer?.[0] ?? 'You')
              : game.opponent[0].toUpperCase()}
          </span>
          <div>
            <strong>
              {topColour !== game.colour ? (samplePlayer ?? 'You') : game.opponent}
            </strong>
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
            <h2>
              {samplePlayer ?? 'You'} vs {game.opponent}
            </h2>
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
                <Check size={13} />
                Engine checked
              </span>
            )}
          </div>
          {!analysis ? (
            <div className="analysis-prompt">
              <ScanSearch size={24} />
              <h3>{busy ? 'Your review is in progress.' : 'Go beyond the result.'}</h3>
              <p>
                {busy
                  ? `${progress}. This game’s findings will appear when it has been checked.`
                  : 'Find up to three positions worth a closer look.'}
              </p>
              <Button className="primary-button" onClick={onAnalyse} disabled={busy}>
                {busy ? <LoaderCircle className="spin" /> : <ScanSearch />}
                {busy ? 'Review in progress…' : 'Analyse this game'}
              </Button>
            </div>
          ) : analysis.findings.length === 0 ? (
            <div className="analysis-prompt">
              <Check size={24} />
              <h3>No large swings confirmed.</h3>
              <p>
                This search did not confirm a two-pawn loss or a lost forced mate on{' '}
                {samplePlayer ? `${samplePlayer}’s` : 'your'} moves. Keep exploring: a
                bounded review can miss mistakes.
              </p>
            </div>
          ) : (
            <>
              <div className="moment-tabs" role="group" aria-label="Go to a key moment">
                {analysis.findings.map((f) => (
                  <Button
                    key={f.ply}
                    variant="ghost"
                    className={finding?.ply === f.ply ? 'active' : ''}
                    aria-pressed={finding?.ply === f.ply}
                    onClick={() => inspect(f)}
                  >
                    {Math.ceil(f.ply / 2)}
                    {game.colour === 'w' ? '.' : '…'} {f.actual}
                    <span>
                      {evaluationChange(f.before, f.after).kind === 'pawns'
                        ? `−${(f.loss! / 100).toFixed(1)}`
                        : 'Mate'}
                    </span>
                  </Button>
                ))}
              </div>
              {finding && !practicing ? (
                <div className="finding-card">
                  <div className="finding-context">
                    Move {Math.ceil(finding.ply / 2)} ·{' '}
                    {current.mode === 'before'
                      ? samplePlayer
                        ? 'Before the move'
                        : 'Before your move'
                      : current.mode === 'played'
                        ? `After ${finding.actual}`
                        : current.mode === 'refutation'
                          ? 'The opponent’s continuation'
                          : 'Exploring the better line'}
                  </div>
                  <h4>{finding.title}</h4>
                  <p>{finding.explanation}</p>
                  <div className="eval-comparison">
                    <div>
                      <span>{samplePlayer ? 'Before the move' : 'Before your move'}</span>
                      <strong>{evalLabel(finding.before)}</strong>
                    </div>
                    <span className="eval-drop">
                      {evaluationChange(finding.before, finding.after).label}
                    </span>
                    <div>
                      <span>After {finding.actual}</span>
                      <strong>{evalLabel(finding.after)}</strong>
                    </div>
                  </div>
                  {inLine && (
                    <div className="variation-line" aria-label="Verified continuation">
                      {frames.slice(1).map((frame, i) => (
                        <button
                          key={i}
                          className={linePly === i + 1 ? 'current-variation' : ''}
                          aria-current={linePly === i + 1 ? 'step' : undefined}
                          onClick={() => navigate({ ...current, linePly: i + 1 })}
                        >
                          {frame.san}
                        </button>
                      ))}
                    </div>
                  )}
                  <p className="evaluation-note">
                    Scores are from {samplePlayer ? `${samplePlayer}’s` : 'your'} side. +1
                    means about a pawn’s advantage; M means a forced mate in the shown
                    number of moves.
                  </p>
                  {finding.clockSecs != null && (
                    <p className="finding-clock">
                      Time remaining: {Math.floor(finding.clockSecs / 60)}:
                      {String(Math.floor(finding.clockSecs % 60)).padStart(2, '0')}
                    </p>
                  )}
                </div>
              ) : practicing ? (
                <div className="finding-card exercise-note">
                  <h4>Make your own observation first.</h4>
                  <p>
                    The explanation is tucked away while you choose. Try a move on the
                    board, ask for a hint, or reveal the verified continuation.
                  </p>
                </div>
              ) : (
                <div className="finding-card position-context">
                  <h4>
                    {ply === 0
                      ? 'The game begins here.'
                      : `You’re viewing move ${Math.ceil(ply / 2)}.`}
                  </h4>
                  <p>
                    Pick a marked key moment above to compare the played move with the
                    verified alternative. The arrows continue through the original game.
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
          <div className="move-list" aria-label="Game moves" ref={moveList}>
            {rows.map((row) => (
              <Fragment key={row.number}>
                <span className="move-number">{row.number}.</span>
                {[row.white, row.black].map((entry, side) =>
                  entry ? (
                    <Button
                      key={side}
                      variant="ghost"
                      ref={!inLine && ply === entry.ply ? currentMove : undefined}
                      className={!inLine && ply === entry.ply ? 'current-move' : ''}
                      aria-current={!inLine && ply === entry.ply ? 'step' : undefined}
                      onClick={() =>
                        navigate(positionAtPly(entry.ply, analysis?.findings))
                      }
                      aria-label={`Go to move ${row.number}, ${side === 0 ? 'White' : 'Black'}, ${entry.move.san}${analysis?.findings.some((f) => f.ply === entry.ply) ? ', key moment' : ''}`}
                    >
                      {entry.move.san}
                      {analysis?.findings.some((f) => f.ply === entry.ply) && (
                        <i className="move-dot" aria-hidden="true" />
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
