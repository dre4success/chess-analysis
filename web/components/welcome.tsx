'use client';
import { useMemo, useState } from 'react';
import { Chess } from 'chess.js';
import {
  ArrowRight,
  ArrowUpRight,
  Check,
  ChessKnight,
  Clock3,
  LoaderCircle,
  MoveUpRight,
  ShieldCheck,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Board } from '@/components/game-review';
import type { ReviewSummary } from '@/lib/api';
import { USERNAME_PATTERN } from '@/lib/chess';

// From the saved native Stockfish review in public/example/review.json.
const exampleFen = '8/1pqbk1p1/p1p1p2p/P1PpP1P1/1P1P3P/5QP1/6B1/6K1 w - - 2 31';
export default function Welcome({
  onConnect,
  onExample,
  busy,
  error,
  savedReviews,
  onResume,
}: {
  onConnect: (name: string) => void;
  onExample: () => void;
  busy: boolean;
  error: string;
  savedReviews: ReviewSummary[];
  onResume: (id: number) => void;
}) {
  const [username, setUsername] = useState('');
  const [variation, setVariation] = useState<'position' | 'played' | 'better'>(
    'position',
  );
  const position = useMemo(() => {
    const board = new Chess(exampleFen);
    if (variation === 'position') return { fen: exampleFen, squares: ['g5'] };
    const move = board.move(variation === 'played' ? 'g6' : 'gxh6');
    return { fen: board.fen(), squares: [move.from, move.to] };
  }, [variation]);
  return (
    <div className="app-shell welcome-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label="Tempo home">
          <span className="brand-mark">
            <ChessKnight size={26} />
          </span>
          tempo<span className="brand-dot">.</span>
        </a>
        <span className="top-note">A space to understand your chess.</span>
        <a className="help-link" href="#how-it-works">
          The process <ArrowUpRight size={15} />
        </a>
      </header>
      <main>
        <div className="onboarding">
          <section className="welcome">
            <div className="eyebrow">
              <span className="live-dot" /> Your personal chess studio
            </div>
            <h1>
              Find your
              <br />
              next <span>move.</span>
              <MoveUpRight className="headline-arrow" aria-hidden="true" />
            </h1>
            <p className="welcome-copy">
              There’s a better player in your last game.
              <br className="desktop-break" /> Let’s find them.
            </p>
            <p className="welcome-detail">
              Your games, the moments that changed them, and a clearer idea of what to
              work on next.
            </p>
            <form
              className="connect-form"
              onSubmit={(e) => {
                e.preventDefault();
                onConnect(username);
              }}
            >
              <label htmlFor="username">Your Chess.com username</label>
              <div className="connect-row">
                <div className="input-wrap">
                  <span className="input-at">@</span>
                  <Input
                    id="username"
                    placeholder="Your username"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    required
                    maxLength={64}
                    pattern={USERNAME_PATTERN}
                    autoComplete="username"
                    autoCapitalize="none"
                    spellCheck={false}
                    disabled={busy}
                  />
                </div>
                <Button className="primary-button" type="submit" disabled={busy}>
                  {busy ? <LoaderCircle className="spin" /> : <ArrowRight size={22} />}
                  <span>{busy ? 'Importing…' : 'Find my games'}</span>
                </Button>
              </div>
              {error && (
                <p className="error-message" role="alert">
                  {error}
                </p>
              )}
              {busy && (
                <p className="import-status" role="status">
                  Importing your recent archives. Your review will continue on the server.
                </p>
              )}
              <p className="privacy-note">
                <ShieldCheck size={14} /> No password. Just your completed, public games.
              </p>
            </form>
            <Button
              variant="ghost"
              className="example-button"
              onClick={onExample}
              disabled={busy}
            >
              Take a look around first <ArrowUpRight size={17} />
            </Button>
            <div className="studio-signature">
              <span className="signature-line" />
              <span>Small discoveries. Stronger chess.</span>
            </div>
          </section>
          <section
            className="preview-stage"
            aria-label="Interactive example from a completed game"
          >
            <div className="preview-caption">
              <span className="eyebrow">A real game, already reviewed</span>
              <span className="pill">Move 31</span>
            </div>
            <div className="preview-board">
              <div className="board-person">
                <span className="mini-avatar">M</span>
                <div>
                  <strong>mhelteo</strong>
                  <span>916 · Black</span>
                </div>
                <span className="board-match-label">Rapid · 30 min</span>
              </div>
              <Board fen={position.fen} highlights={position.squares} />
              <div className="board-person">
                <span className="mini-avatar peach">D</span>
                <div>
                  <strong>dre4success007</strong>
                  <span>1020 · White</span>
                </div>
                <span className="clock">
                  <Clock3 size={14} />
                  07:53
                </span>
              </div>
            </div>
            <div className="preview-insight">
              <div className="insight-heading">
                <span className="insight-icon">
                  <ChessKnight size={21} />
                </span>
                <div>
                  <span className="eyebrow">A missed opportunity</span>
                  <h3>One capture. More possibilities.</h3>
                </div>
                <Check size={17} />
              </div>
              <p>
                Capturing on h6 keeps a much stronger advantage. Compare the moves on the
                board.
              </p>
              <div className="preview-choices" aria-label="Compare moves in the example">
                <Button
                  variant="ghost"
                  aria-pressed={variation === 'position'}
                  onClick={() => setVariation('position')}
                >
                  Position
                </Button>
                <Button
                  variant="ghost"
                  aria-pressed={variation === 'played'}
                  onClick={() => setVariation('played')}
                >
                  Played <b>g6</b>
                </Button>
                <Button
                  variant="ghost"
                  aria-pressed={variation === 'better'}
                  onClick={() => setVariation('better')}
                >
                  Try <b>gxh6</b>
                  <ArrowUpRight size={14} />
                </Button>
              </div>
            </div>
            <div className="preview-foot">
              <span>From a saved Stockfish review</span>
              <button onClick={onExample} disabled={busy}>
                Open the full review <ArrowUpRight size={14} />
              </button>
            </div>
          </section>
        </div>
        {savedReviews.length > 0 && (
          <section className="saved-reviews">
            <div className="section-heading">
              <h2>Your review shelf</h2>
              <span className="subtle-chip">Saved on this server</span>
            </div>
            <div className="saved-grid">
              {savedReviews.slice(0, 4).map((review) => (
                <button
                  key={review.id}
                  className="saved-review"
                  onClick={() => onResume(review.id)}
                  disabled={busy}
                >
                  <span className="mini-avatar peach">
                    {review.username[0].toUpperCase()}
                  </span>
                  <span>
                    <strong>{review.username}</strong>
                    <small>
                      <span className="chip-pace">{review.pace}</span> ·{' '}
                      {review.status === 'complete'
                        ? `${review.progress.completed} games reviewed`
                        : review.status}{' '}
                      ·{' '}
                      {new Date(review.created_at).toLocaleDateString(undefined, {
                        day: 'numeric',
                        month: 'short',
                      })}
                    </small>
                  </span>
                  <ArrowUpRight size={18} />
                </button>
              ))}
            </div>
          </section>
        )}
        <section className="welcome-steps" id="how-it-works" aria-label="How Tempo works">
          <div>
            <span className="step-number">01</span>
            <div>
              <h3>Bring your games.</h3>
              <p>Connect your username. We’ll pull your recent play from Chess.com.</p>
            </div>
          </div>
          <div>
            <span className="step-number">02</span>
            <div>
              <h3>Meet the turning points.</h3>
              <p>Stockfish checks your moves and finds positions worth a second look.</p>
            </div>
          </div>
          <div>
            <span className="step-number">03</span>
            <div>
              <h3>Play with perspective.</h3>
              <p>Explore a better line. Take one useful idea into your next game.</p>
            </div>
          </div>
        </section>
      </main>
      <footer className="site-footer">
        <span>For the player you’re becoming.</span>
        <span>Independent of Chess.com.</span>
      </footer>
    </div>
  );
}
