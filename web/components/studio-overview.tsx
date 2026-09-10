import { lazy, Suspense, useMemo, useState } from 'react';
import {
  ArrowUpRight,
  ArrowRight,
  ChessKnight,
  CheckCheck,
  Layers,
  Sparkles,
  Target,
  Download,
  ChevronDown,
  Clock3,
} from 'lucide-react';
import { Board } from './game-review';
import RatingPanelBoundary from './rating-panel-boundary';
import { loadModule } from '../lib/load-module';
import {
  dateLabel,
  summarise,
  timeLabel,
  type Game,
  type GameAnalysis,
  type Pace,
} from '../lib/chess';
import { themesFor, studyPriority } from '../lib/coaching';
const RatingPanel = lazy(() => loadModule(() => import('./rating-panel')));
export default function StudioOverview({
  games,
  analyses,
  pace,
  onReview,
  digestUrl,
  busy,
  onContinue,
  canContinue,
}: {
  games: Game[];
  analyses: Record<string, GameAnalysis>;
  pace: Pace;
  onReview: (game: Game, ply?: number) => void;
  digestUrl?: string;
  busy: boolean;
  onContinue: () => void;
  canContinue: boolean;
}) {
  const [showAll, setShowAll] = useState(false);
  const stats = useMemo(() => summarise(games), [games]);
  const themes = useMemo(() => themesFor(games, analyses), [games, analyses]);
  const priority = useMemo(() => studyPriority(games, analyses), [games, analyses]);
  const checked = games.filter((g) => analyses[g.id]);
  const moments = checked.flatMap((g) => analyses[g.id].findings);
  const graph = games.toReversed().map((g, i) => ({
    index: i + 1,
    rating: g.rating,
    date: dateLabel(g.date),
    opponent: g.opponent,
    result: g.result === 'win' ? 'Won' : g.result === 'draw' ? 'Drew' : 'Lost',
  }));
  const theme = themes[0];
  const focusGame = priority?.game ?? games[0];
  const focus = priority?.finding;
  const focusFen = focus?.beforeFen ?? focusGame.moves[0].before;
  const counts = [stats.wins, stats.draw, stats.loss];
  const labels = ['Wins', 'Draws', 'Losses'];
  const colours = ['#397e70', '#bbacd0', '#e99b7d'];
  let offset = 0;
  return (
    <div className="overview-content">
      <div className="overview-metrics">
        <div>
          <span className="metric-icon lavender">
            <Layers size={20} />
          </span>
          <span>
            <strong>{games.length}</strong>
            <small>games in this chapter</small>
          </span>
          <span className="metric-foot">
            {dateLabel(games.at(-1)!.date)} – {dateLabel(games[0].date)}
          </span>
        </div>
        <div>
          <span className="metric-icon mint">
            <CheckCheck size={20} />
          </span>
          <span>
            <strong>
              {checked.length}
              <i>/{games.length}</i>
            </strong>
            <small>games checked</small>
          </span>
          <span className="metric-foot">
            {busy ? 'Review in progress' : 'Saved & ready'}
          </span>
        </div>
        <div>
          <span className="metric-icon peach">
            <Target size={20} />
          </span>
          <span>
            <strong>{moments.length}</strong>
            <small>moments to explore</small>
          </span>
          <span className="metric-foot">Up to 3 per checked game</span>
        </div>
        <div>
          <span className="metric-icon lemon">
            <Sparkles size={20} />
          </span>
          <span>
            <strong>{themes.filter((t) => t.affected >= 2).length}</strong>
            <small>repeated themes</small>
          </span>
          <span className="metric-foot">Across checked games</span>
        </div>
      </div>
      <div className="studio-main-grid">
        <RatingPanelBoundary>
          <Suspense
            fallback={
              <div className="panel rating-panel skeleton-chart" role="status">
                Drawing your rating journey…
              </div>
            }
          >
            <RatingPanel graph={graph} pace={pace} change={stats.change} />
          </Suspense>
        </RatingPanelBoundary>
        <section className="panel results-panel">
          <div className="section-heading">
            <div>
              <span className="eyebrow">HOW THE GAMES WENT</span>
              <h2>The shape of your play</h2>
            </div>
          </div>
          <div className="result-donut">
            <svg
              viewBox="0 0 200 200"
              role="img"
              aria-label={`${stats.wins} wins, ${stats.draw} draws, ${stats.loss} losses from ${games.length} games`}
            >
              <circle
                cx="100"
                cy="100"
                r="77"
                fill="none"
                stroke="#efeae2"
                strokeWidth="17"
              />
              {counts.map((count, i) => {
                const length = (count / games.length) * 483.8;
                const start = offset;
                offset += length;
                return count > 0 ? (
                  <circle
                    key={labels[i]}
                    cx="100"
                    cy="100"
                    r="77"
                    fill="none"
                    stroke={colours[i]}
                    strokeWidth="17"
                    strokeDasharray={`${Math.max(0, length - 5)} ${483.8 - Math.max(0, length - 5)}`}
                    strokeDashoffset={-start}
                    transform="rotate(-90 100 100)"
                  />
                ) : null;
              })}
            </svg>
            <div>
              <strong>
                {stats.winRate}
                <span>%</span>
              </strong>
              <small>WIN RATE</small>
            </div>
          </div>
          <div className="result-legend">
            {counts.map((n, i) => (
              <div key={labels[i]}>
                <span>
                  <i style={{ background: colours[i] }} />
                  {labels[i]}
                </span>
                <strong>{n}</strong>
              </div>
            ))}
          </div>
          <p className="sample-note">
            Your whole imported sample. Every result adds context.
          </p>
        </section>
        <section className="practice-focus">
          <div className="focus-copy">
            <span className="eyebrow">
              <Sparkles size={15} /> YOUR NEXT SMALL BREAKTHROUGH
            </span>
            <h2>
              {theme?.affected && theme.affected >= 2
                ? theme.title
                : 'A better move starts with a question.'}
            </h2>
            <p>
              {theme && theme.affected >= 2
                ? theme.action
                : focus
                  ? 'Look at checks, captures and threats. Try your idea before you explore the engine’s line.'
                  : 'Start with one completed game. The turning points will appear as analysis finishes.'}
            </p>
            <div className="focus-evidence">
              {theme && theme.affected >= 2 ? (
                <>
                  <span className="evidence-number">
                    {theme.affected}
                    <small>/{theme.reviewed}</small>
                  </span>
                  <span>
                    checked games share this theme.
                    <br />A useful clue for your next session.
                  </span>
                </>
              ) : (
                <>
                  <ChessKnight size={24} />
                  <span>
                    {focus
                      ? `Move ${Math.ceil(focus.ply / 2)} against ${focusGame.opponent}`
                      : 'One position. A little curiosity.'}
                  </span>
                </>
              )}
            </div>
            <button
              className="ink-button"
              onClick={() => onReview(focusGame, focus?.ply)}
            >
              {focus ? 'Step into the position' : 'Review your latest game'}
              <ArrowUpRight size={19} />
            </button>
          </div>
          <button
            className="focus-board"
            onClick={() => onReview(focusGame, focus?.ply)}
            aria-label="Open the featured position in game review"
          >
            <div className="focus-board-kicker">
              <span>{focusFen.split(' ')[1] === 'w' ? 'WHITE' : 'BLACK'} TO MOVE</span>
              <ArrowUpRight size={15} />
            </div>
            <Board fen={focusFen} flipped={focusGame.colour === 'b'} />
            <span className="focus-board-caption">
              {focus ? `Before ${focus.actual}` : 'The starting position'} ·{' '}
              {focusGame.opponent}
            </span>
          </button>
        </section>
      </div>
      {themes.length > 0 && (
        <section className="themes-section">
          <div className="section-heading">
            <div>
              <span className="eyebrow">CONNECTING THE DOTS</span>
              <h2>Patterns worth a second look</h2>
            </div>
            <span className="subtle-chip">{checked.length} games checked</span>
          </div>
          <div className="theme-grid">
            {themes.slice(0, 3).map((t, i) => (
              <button
                className="theme-card"
                key={t.classification}
                onClick={() => onReview(t.examples[0].game, t.examples[0].finding.ply)}
              >
                <div>
                  <span className={`theme-number theme-number-${i}`}>0{i + 1}</span>
                  <span className="theme-incidence">
                    {t.affected}/{t.reviewed} games
                  </span>
                  <ArrowUpRight size={17} />
                </div>
                <h3>{t.title}</h3>
                <p>{t.action}</p>
                <div className="theme-track">
                  <span style={{ width: `${(t.affected / t.reviewed) * 100}%` }} />
                </div>
                <small>
                  {t.affected < 2
                    ? 'One observation — keep exploring.'
                    : `${t.examples.length} selected moments across ${t.affected} games.`}
                </small>
              </button>
            ))}
          </div>
          <p className="sample-note">
            Themes describe selected, engine-confirmed moments. Small samples are clues;
            they don’t measure every mistake.
          </p>
        </section>
      )}
      <div className="studio-bottom-grid">
        <section className="panel games-panel">
          <div className="section-heading">
            <div>
              <span className="eyebrow">THE SOURCE MATERIAL</span>
              <h2>Your recent games</h2>
            </div>
            <span className="subtle-chip">{pace}</span>
          </div>
          <div className="game-table-wrap">
            <table className="game-table">
              <thead>
                <tr>
                  <th>Opponent</th>
                  <th>Result</th>
                  <th className="table-opening">Opening</th>
                  <th>Review</th>
                </tr>
              </thead>
              <tbody>
                {(showAll ? games : games.slice(0, 6)).map((g) => (
                  <tr key={g.id}>
                    <td>
                      <button
                        className="opponent-button"
                        aria-label={`Review the game against ${g.opponent}`}
                        onClick={() => onReview(g)}
                      >
                        <span
                          className={`opponent-monogram ${g.colour === 'b' ? 'dark-monogram' : ''}`}
                        >
                          {g.opponent[0].toUpperCase()}
                        </span>
                        <span>
                          <strong>{g.opponent}</strong>
                          <small>
                            {g.opponentRating} · {dateLabel(g.date)} ·{' '}
                            {timeLabel(g.timeControl)}
                          </small>
                        </span>
                      </button>
                    </td>
                    <td>
                      <span className={`result-chip result-${g.result}`}>
                        {g.result === 'win'
                          ? 'Won'
                          : g.result === 'loss'
                            ? 'Lost'
                            : 'Drew'}
                      </span>
                    </td>
                    <td className="table-opening">
                      <span>{g.opening}</span>
                      <small>{g.eco}</small>
                    </td>
                    <td>
                      <button
                        className="game-open"
                        aria-label={`Review game against ${g.opponent}`}
                        onClick={() => onReview(g)}
                      >
                        <span>
                          {analyses[g.id]
                            ? `${analyses[g.id].findings.length} moments`
                            : 'Explore'}
                        </span>
                        <ArrowUpRight size={17} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {games.length > 6 && (
            <button
              className="table-more"
              aria-expanded={showAll}
              onClick={() => setShowAll(!showAll)}
            >
              {showAll ? 'Show recent six' : `See all ${games.length} games`}
              <ChevronDown size={15} className={showAll ? 'rotate' : ''} />
            </button>
          )}
        </section>
        <section className="panel openings-panel">
          <div className="section-heading">
            <div>
              <span className="eyebrow">YOUR OPENING PALETTE</span>
              <h2>Familiar territory</h2>
            </div>
          </div>
          <p className="panel-subtitle">The positions you return to most.</p>
          <div className="opening-list">
            {stats.openings.slice(0, 5).map((o) => (
              <div className="opening-row" key={`${o.colour}:${o.name}`}>
                <div className="opening-title">
                  <span
                    className={`colour-dot ${o.colour === 'w' ? 'white-dot' : 'black-dot'}`}
                  />
                  <strong>{o.name}</strong>
                  <small>
                    {o.games} {o.games === 1 ? 'game' : 'games'}
                  </small>
                </div>
                <div
                  className="opening-bar"
                  role="img"
                  aria-label={`${o.wins} wins, ${o.draws} draws, ${o.games - o.wins - o.draws} losses`}
                >
                  {[o.wins, o.draws, o.games - o.wins - o.draws].map((n, i) =>
                    n > 0 ? (
                      <span
                        key={labels[i]}
                        style={{
                          width: `${(n / o.games) * 100}%`,
                          background: colours[i],
                        }}
                      />
                    ) : null,
                  )}
                </div>
              </div>
            ))}
          </div>
          <div className="bar-legend">
            {labels.map((l, i) => (
              <span key={l}>
                <i style={{ background: colours[i] }} />
                {l}
              </span>
            ))}
          </div>
          <p className="sample-note">
            Results give context. They don’t prove an opening caused a win or loss.
          </p>
        </section>
      </div>
      <div className="studio-takeaway">
        <span className="takeaway-icon">
          <BookIcon />
        </span>
        <div>
          <h3>Take the insight with you.</h3>
          <p>
            Keep a concise digest of checked positions for your next coaching session.
          </p>
        </div>
        {digestUrl ? (
          <a className="secondary-button" href={digestUrl} download>
            <Download size={17} /> Download coaching digest
          </a>
        ) : (
          <span className="subtle-chip">
            <Clock3 size={14} /> Available after a saved game is checked
          </span>
        )}
      </div>
      {!busy && canContinue && (
        <button className="secondary-button continue-button" onClick={onContinue}>
          Continue checking this chapter <ArrowRight size={17} />
        </button>
      )}
    </div>
  );
}
function BookIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      width="26"
      height="26"
      aria-hidden="true"
    >
      <path d="M3 4h6a3 3 0 0 1 3 3v14a3 3 0 0 0-3-3H3V4Zm18 0h-6a3 3 0 0 0-3 3v14a3 3 0 0 1 3-3h6V4Z" />
    </svg>
  );
}
