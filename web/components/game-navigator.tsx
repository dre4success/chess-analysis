'use client';
import { useEffect, useRef, useState } from 'react';
import { Check, ChevronLeft, ChevronRight } from 'lucide-react';
import { dateLabel, type Game, type GameAnalysis } from '@/lib/chess';

export default function GameNavigator({
  games,
  analyses,
  selectedId,
  onSelect,
  sample = false,
}: {
  games: Game[];
  analyses: Record<string, GameAnalysis>;
  selectedId: string;
  onSelect: (id: string) => void;
  sample?: boolean;
}) {
  const list = useRef<HTMLOListElement>(null);
  const selected = useRef<HTMLButtonElement>(null);
  const [edges, setEdges] = useState({ start: true, end: false });
  const checked = games.filter((game) => analyses[game.id]).length;

  function updateEdges() {
    const node = list.current;
    if (node) {
      const start = node.scrollLeft <= 1;
      const end = node.scrollLeft + node.clientWidth >= node.scrollWidth - 1;
      setEdges((previous) =>
        previous.start === start && previous.end === end ? previous : { start, end },
      );
    }
  }

  useEffect(() => {
    const node = list.current;
    const current = selected.current;
    if (!node || !current) return;
    // Keep the current game visible without moving the page or board.
    const reveal = () => {
      const horizontal = node.scrollWidth > node.clientWidth + 1;
      node.scrollTo({
        left: horizontal
          ? Math.max(0, current.offsetLeft - (node.clientWidth - current.offsetWidth) / 2)
          : 0,
        top: horizontal ? 0 : Math.max(0, current.offsetTop - node.clientHeight / 3),
        behavior: 'instant',
      });
      updateEdges();
    };
    const observer = new ResizeObserver(reveal);
    observer.observe(node);
    reveal();
    return () => observer.disconnect();
  }, [selectedId]);

  function scroll(direction: number) {
    const node = list.current;
    if (!node) return;
    node.scrollBy({
      left: direction * node.clientWidth * 0.8,
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches
        ? 'instant'
        : 'smooth',
    });
  }

  return (
    <nav className="game-rail" aria-label="Games in this review">
      <div className="rail-heading">
        <div className="rail-title-row">
          <h2>
            {sample ? 'Sample games' : 'Your games'} <span>{games.length}</span>
          </h2>
          <div className="rail-scroll-controls">
            <button
              aria-label="Show previous games"
              disabled={edges.start}
              onClick={() => scroll(-1)}
            >
              <ChevronLeft size={18} />
            </button>
            <button
              aria-label="Show next games"
              disabled={edges.end}
              onClick={() => scroll(1)}
            >
              <ChevronRight size={18} />
            </button>
          </div>
        </div>
        <span>
          {checked} of {games.length} checked
        </span>
        <div className="rail-progress" aria-hidden="true">
          <span
            style={{ width: `${games.length ? (checked / games.length) * 100 : 0}%` }}
          />
        </div>
      </div>
      <ol className="rail-list" ref={list} onScroll={updateEdges}>
        {games.map((game) => {
          const analysis = analyses[game.id];
          const found = analysis?.findings.length ?? 0;
          const status = analysis
            ? found
              ? `${found} ${found === 1 ? 'moment' : 'moments'}`
              : 'No key moments'
            : 'Not checked';
          const result =
            game.result === 'win' ? 'Won' : game.result === 'loss' ? 'Lost' : 'Drew';
          const colour = game.colour === 'w' ? 'White' : 'Black';
          const active = game.id === selectedId;
          return (
            <li key={game.id}>
              <button
                className="rail-game"
                ref={active ? selected : undefined}
                aria-current={active ? 'true' : undefined}
                aria-label={`Review game against ${game.opponent}, ${result.toLowerCase()}, ${dateLabel(game.date)}, played as ${colour}, ${analysis ? 'checked, ' : ''}${status.toLowerCase()}`}
                onClick={() => onSelect(game.id)}
              >
                <span className="rail-opponent">
                  <strong>{game.opponent}</strong>
                  {active && <ChevronRight size={17} aria-hidden="true" />}
                </span>
                <span className="rail-game-meta">
                  <span className={`rail-result ${game.result}`}>{result}</span>
                  <time dateTime={new Date(game.date * 1000).toISOString()}>
                    {dateLabel(game.date)}
                  </time>
                </span>
                <span className="rail-game-footer">
                  <span className="rail-colour">
                    <span
                      className={`colour-dot ${game.colour === 'w' ? 'white-dot' : 'black-dot'}`}
                      aria-hidden="true"
                    />
                    {colour}
                  </span>
                  <span className={analysis ? 'rail-moments' : 'rail-pending'}>
                    {analysis && found === 0 ? (
                      <>
                        <Check size={14} aria-hidden="true" /> Checked
                      </>
                    ) : (
                      status
                    )}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
