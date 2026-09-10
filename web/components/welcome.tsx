import { useMemo, useState } from 'react';
import { Chess } from 'chess.js';
import {
  ArrowRight,
  ArrowUpRight,
  BookOpen,
  ChessKnight,
  Clock3,
  Sparkles,
  ShieldCheck,
  LoaderCircle,
} from 'lucide-react';
import { Board } from './game-review';
import { USERNAME_PATTERN, timeLabel } from '../lib/chess';
import type { ReviewSummary } from '../lib/api';
import artwork from '../assets/tempo-still-life.jpg';
import preview from '../assets/example-preview.json';
import exampleProfile from '../assets/example-profile.json';

export default function Welcome({
  onConnect,
  onExample,
  onExamplePosition,
  busy,
  error,
  savedReviews,
  onResume,
  onLibrary,
}: {
  onConnect: (name: string) => void;
  onExample: () => void;
  onExamplePosition: () => void;
  busy: boolean;
  error: string;
  savedReviews: ReviewSummary[];
  onResume: (id: number) => void;
  onLibrary: () => void;
}) {
  const [username, setUsername] = useState('');
  const [choice, setChoice] = useState<'before' | 'played' | 'better'>('before');
  const previewColour = preview.fen.split(' ')[1];
  const previewPlayer = previewColour === 'b' ? preview.black : preview.white;
  const previewOpponent = previewColour === 'b' ? preview.white : preview.black;
  const position = useMemo(() => {
    const c = new Chess(preview.fen);
    if (choice === 'before') return { fen: preview.fen, squares: [] as string[] };
    const m = c.move(choice === 'played' ? preview.actual : preview.best);
    return { fen: c.fen(), squares: [m.from, m.to] };
  }, [choice]);
  return (
    <div className="welcome-shell">
      <header className="topbar welcome-topbar">
        <a className="brand" href="/" aria-label="Tempo home">
          <span className="brand-mark">
            <ChessKnight size={27} />
          </span>
          tempo<span className="brand-period">.</span>
        </a>
        <span className="top-note">A little reflection. A different game.</span>
        <button className="text-button" onClick={onLibrary}>
          <BookOpen size={17} /> Your library <ArrowUpRight size={16} />
        </button>
      </header>
      <main>
        <section className="welcome-hero">
          <div className="welcome-copy-column">
            <div className="eyebrow">
              <span className="small-star">✳</span> THE ART OF GETTING BETTER
            </div>
            <h1>
              Every move
              <br />
              has a <em>story.</em>
            </h1>
            <p className="hero-lede">
              Discover what your games are teaching you.
              <br className="desktop-break" /> Make the next move your best yet.
            </p>
            <form
              className="connect-form"
              onSubmit={(e) => {
                e.preventDefault();
                onConnect(username);
              }}
            >
              <label htmlFor="username">Start with your Chess.com username</label>
              <div className="connect-row">
                <span className="input-at">@</span>
                <input
                  id="username"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="Your username"
                  required
                  pattern={USERNAME_PATTERN}
                  maxLength={64}
                  title="Letters, numbers, underscores and hyphens"
                  autoComplete="username"
                  autoCapitalize="none"
                  spellCheck={false}
                  disabled={busy}
                />
                <button className="primary-button" disabled={busy}>
                  {busy ? (
                    <LoaderCircle className="spin" size={18} />
                  ) : (
                    <ArrowRight size={19} />
                  )}
                  <span>{busy ? 'Finding games…' : 'Find my games'}</span>
                </button>
              </div>
              {error && (
                <p className="error-message" role="alert">
                  {error}
                </p>
              )}
              {busy && (
                <p className="import-status" role="status">
                  Your completed games are on their way. Your review will keep running on
                  the server.
                </p>
              )}
              <p className="privacy-note">
                <ShieldCheck size={14} /> No password. Only completed, public games.
              </p>
            </form>
            <button
              className="example-button text-button"
              onClick={onExample}
              disabled={busy}
            >
              Explore a sample review <ArrowUpRight size={17} />
            </button>
            <div className="hero-signature">
              <span className="signature-orbit">64</span>
              <span>
                Sixty-four squares.
                <br />
                <strong>Endless room to grow.</strong>
              </span>
            </div>
          </div>
          <div className="hero-art">
            <img
              src={artwork}
              alt="Sculptural ivory knight, coral pawn and plum bishop in warm afternoon light"
              width="1122"
              height="1402"
              fetchPriority="high"
            />
            <div className="art-topline">
              <span>THE TEMPO STUDIO</span>
              <span>01 / REFLECTION</span>
            </div>
            <div className="art-caption">
              <span className="caption-icon">
                <Sparkles size={19} />
              </span>
              <span>
                Small discoveries.
                <br />
                <strong>Stronger chess.</strong>
              </span>
              <span className="art-arrow">↗</span>
            </div>
          </div>
        </section>
        {savedReviews.length > 0 && (
          <section className="returning-section">
            <div>
              <span className="eyebrow">YOUR RECENT LOOKUPS</span>
              <h2>Pick up your thread.</h2>
            </div>
            <div className="returning-cards">
              {savedReviews.slice(0, 2).map((r) => (
                <button
                  className="returning-card"
                  key={r.id}
                  onClick={() => onResume(r.id)}
                  disabled={busy}
                >
                  <span className="mini-avatar">{r.username[0].toUpperCase()}</span>
                  <span>
                    <strong>{r.username}</strong>
                    <small>
                      {r.pace} ·{' '}
                      {new Date(r.created_at).toLocaleDateString('en-GB', {
                        day: 'numeric',
                        month: 'short',
                      })}{' '}
                      at{' '}
                      {new Date(r.created_at).toLocaleTimeString('en-GB', {
                        hour: '2-digit',
                        minute: '2-digit',
                      })}
                      <br />
                      Review #{r.id} · {r.reviewed_games ?? r.progress.completed} games
                      checked
                    </small>
                  </span>
                  <ArrowUpRight size={19} />
                </button>
              ))}
            </div>
            <button className="text-button" onClick={onLibrary}>
              All saved reviews <ArrowRight size={16} />
            </button>
          </section>
        )}
        <section className="welcome-process" aria-label="How Tempo works">
          <div>
            <span className="step-number">01</span>
            <h3>Bring your games.</h3>
            <p>Your recent play, gathered in one beautiful place.</p>
          </div>
          <div>
            <span className="step-number">02</span>
            <h3>Find the turning points.</h3>
            <p>Engine-checked positions. Patterns you can understand.</p>
          </div>
          <div>
            <span className="step-number">03</span>
            <h3>Take an idea with you.</h3>
            <p>Try a move. Follow the line. Build one better habit.</p>
          </div>
        </section>
        <section className="welcome-example">
          <div className="example-editorial">
            <span className="eyebrow">
              SAMPLE REVIEW · {exampleProfile.name.toUpperCase()}
            </span>
            <h2>
              What would
              <br />
              <em>you play?</em>
            </h2>
            <p>
              An engine-checked moment from {exampleProfile.name}’s public games. Compare
              the moves, then explore this exact position in the study room.
            </p>
            <div className="example-pills">
              <span>
                <Clock3 size={14} />
                {timeLabel(preview.timeControl)}
              </span>
              <span>Move {preview.move}</span>
              <span>{previewColour === 'b' ? 'Black' : 'White'} to move</span>
            </div>
            <div className="preview-choices">
              {(['before', 'played', 'better'] as const).map((c) => (
                <button
                  aria-pressed={choice === c}
                  className={choice === c ? 'selected' : ''}
                  key={c}
                  onClick={() => setChoice(c)}
                >
                  {c === 'before'
                    ? 'Before'
                    : c === 'played'
                      ? `Played ${preview.actual}`
                      : `Better ${preview.best}`}
                </button>
              ))}
            </div>
            <button
              className="primary-button"
              onClick={onExamplePosition}
              disabled={busy}
            >
              Explore this sample position <ArrowUpRight size={18} />
            </button>
          </div>
          <div className="example-board-wrap">
            <div className="example-board-label">
              <span>{previewOpponent.username}</span>
              <span>{previewOpponent.rating}</span>
            </div>
            <Board
              fen={position.fen}
              highlights={position.squares}
              flipped={previewColour === 'b'}
            />
            <div className="example-board-label">
              <span>{previewPlayer.username}</span>
              <span>{previewPlayer.rating}</span>
            </div>
          </div>
        </section>
      </main>
      <footer className="site-footer">
        <a className="brand" href="/">
          tempo.
        </a>
        <span>Made for the player you’re becoming.</span>
        <span>Independent of Chess.com.</span>
      </footer>
    </div>
  );
}
