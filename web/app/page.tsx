'use client';
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowDownRight,
  ArrowRight,
  ArrowUpRight,
  ChartNoAxesCombined,
  Check,
  ChessKnight,
  CircleHelp,
  Clock3,
  Layers,
  LoaderCircle,
  RefreshCw,
  Target,
  TrendingUp,
  X,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import Welcome from '@/components/welcome';
import GameReview, { Board } from '@/components/game-review';
import RatingPanelBoundary from '@/components/rating-panel-boundary';
import { loadModule } from '@/lib/load-module';
import {
  dateLabel,
  normaliseUsername,
  parseGame,
  summarise,
  timeLabel,
  type Game,
  type GameAnalysis,
  type Pace,
  type PlayerData,
} from '@/lib/chess';
import { convertFinding, type SavedReview } from '@/lib/review-data';
import {
  api,
  mappedAnalyses,
  running,
  watchReview,
  type ReviewJob,
  type ReviewRequest,
  type ReviewSummary,
} from '@/lib/api';
const RatingPanel = lazy(() => loadModule(() => import('@/components/rating-panel')));
export default function Home() {
  const [data, setData] = useState<PlayerData | null>(null),
    [pace, setPace] = useState<Pace>('rapid'),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [example, setExample] = useState(false),
    [view, setView] = useState<'overview' | 'review'>('overview'),
    [selected, setSelected] = useState(''),
    [selectedPly, setSelectedPly] = useState<number | undefined>(),
    [analyses, setAnalyses] = useState<Record<string, GameAnalysis>>({}),
    [engineBusy, setEngineBusy] = useState(false),
    [engineProgress, setEngineProgress] = useState(''),
    [engineError, setEngineError] = useState(''),
    [showAll, setShowAll] = useState(false),
    [savedReviews, setSavedReviews] = useState<ReviewSummary[]>([]),
    [jobId, setJobId] = useState<number | null>(null);
  const currentJob = useRef<number | null>(null);
  const request = useRef<AbortController | null>(null),
    generation = useRef(0);
  const games = useMemo(
    () =>
      data
        ? data.games
            .map((g) => parseGame(g, data.profile.username.toLowerCase()))
            .filter((g): g is Game => !!g)
        : [],
    [data],
  );
  const stats = useMemo(() => summarise(games), [games]);
  useEffect(
    () => () => {
      request.current?.abort();
    },
    [],
  );
  const follow = useCallback(async (jobId: number, controller: AbortController) => {
    currentJob.current = jobId;
    setJobId(jobId);
    window.history.replaceState({}, '', `?review=${jobId}`);
    try {
      let lastReview = '';
      for await (const job of watchReview(jobId, controller.signal)) {
        setEngineBusy(running(job));
        setEngineProgress(
          `${job.message}${job.progress.positions ? ` · ${job.progress.positions} positions checked` : ''}`,
        );
        if (job.data) {
          setPace(job.pace);
          setExample(false);
          const next = job.data;
          setData((old) =>
            old?.fetchedAt === next.fetchedAt &&
            old.profile.username === next.profile.username &&
            old.pace === next.pace
              ? old
              : next,
          );
          setSelected((old) =>
            next.games.some((g) => g.url === old) ? old : (next.games[0]?.url ?? ''),
          );
          setBusy(false);
        }
        const reviewVersion = JSON.stringify(job.review);
        if (job.review && lastReview !== reviewVersion) {
          setAnalyses(mappedAnalyses(job));
          lastReview = reviewVersion;
        }
        if (job.status === 'failed') {
          if (job.data) setEngineError(job.error || job.message);
          else setError(job.error || job.message);
        }
        if (!running(job)) setBusy(false);
      }
    } catch (e) {
      if (!controller.signal.aborted) {
        setEngineError(
          `Connection interrupted. Review #${jobId} is saved on the server. Reconnect to check its progress.`,
        );
        setError(e instanceof Error ? e.message : 'Could not reach the review server.');
        setBusy(false);
        setEngineBusy(false);
      }
    }
  }, []);
  const resumeJob = useCallback(
    (id: number) => {
      generation.current++;
      request.current?.abort();
      const controller = new AbortController();
      request.current = controller;
      setError('');
      setEngineError('');
      setBusy(true);
      setAnalyses({});
      setView('overview');
      void follow(id, controller);
    },
    [follow],
  );
  const startJob = useCallback(
    async (input: ReviewRequest, clear: boolean) => {
      generation.current++;
      request.current?.abort();
      const controller = new AbortController();
      request.current = controller;
      setBusy(true);
      setError('');
      setEngineError('');
      try {
        const job = await api<ReviewJob>('/reviews', controller.signal, input);
        if (controller.signal.aborted) return null;
        if (clear) {
          setAnalyses({});
          setView('overview');
          setShowAll(false);
        }
        void follow(job.id, controller);
        return { username: job.username, reviewId: job.id, pace: job.pace };
      } catch (e) {
        if (!controller.signal.aborted) {
          setBusy(false);
          setError((e as Error).message);
        }
        return null;
      }
    },
    [follow],
  );
  const connect = useCallback(
    async (name: string, nextPace: Pace = 'rapid') => {
      try {
        return await startJob(
          { username: normaliseUsername(name), pace: nextPace },
          true,
        );
      } catch (e) {
        setError((e as Error).message);
        return null;
      }
    },
    [startJob],
  );
  async function runAnalysis(batch: Game[]) {
    if (!data || !batch.length) return;
    if (currentJob.current === null) {
      await connect(data.profile.username, pace);
      return;
    }
    await startJob(
      {
        username: data.profile.username,
        pace,
        parent_id: currentJob.current,
        game_urls: batch.map((g) => g.id),
      },
      false,
    );
  }
  async function cancelAnalysis() {
    if (currentJob.current === null) return;
    try {
      await api(`/reviews/${currentJob.current}/cancel`, undefined, {});
    } catch (e) {
      setEngineError((e as Error).message);
    }
  }
  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get('review');
    const controller = new AbortController();
    if (id && /^[1-9][0-9]*$/.test(id)) {
      void Promise.resolve().then(() => {
        if (!controller.signal.aborted) resumeJob(Number(id));
      });
    }
    return () => controller.abort();
  }, [resumeJob]);
  useEffect(() => {
    if (data) return;
    const controller = new AbortController();
    api<{ reviews: ReviewSummary[] }>('/reviews', controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) setSavedReviews(result.reviews);
      })
      .catch(() => {});
    return () => controller.abort();
  }, [data]);
  async function loadExample() {
    const id = ++generation.current;
    request.current?.abort();
    currentJob.current = null;
    setJobId(null);
    window.history.replaceState({}, '', '/');
    setBusy(true);
    setError('');
    setEngineBusy(false);
    try {
      const [g, r] = await Promise.all([
        fetch('/example/games.json').then((r) => {
          if (!r.ok) throw new Error('The example could not be loaded.');
          return r.json() as Promise<{ games: PlayerData['games'] }>;
        }),
        fetch('/example/review.json').then((r) => {
          if (!r.ok) throw new Error('The example review could not be loaded.');
          return r.json() as Promise<SavedReview>;
        }),
      ]);
      if (id !== generation.current) return;
      const next: PlayerData = {
        profile: { username: r.user },
        games: g.games.sort((a, b) => b.end_time - a.end_time),
        pace: 'rapid',
        fetchedAt: r.generated,
        archivesRead: 1,
      };
      const mapped: Record<string, GameAnalysis> = {};
      for (const game of r.games) {
        mapped[game.url] = {
          id: game.url,
          source: 'verified',
          positions: 0,
          findings: game.findings.map(convertFinding),
        };
      }
      if (id !== generation.current) return;
      setData(next);
      setAnalyses(mapped);
      setPace('rapid');
      setExample(true);
      setView('overview');
      setShowAll(false);
      setEngineError('');
      setSelected(next.games[0]?.url ?? '');
    } catch (e) {
      if (id === generation.current) setError((e as Error).message);
    } finally {
      if (id === generation.current) setBusy(false);
    }
  }
  useEffect(() => {
    const context = (
      document as unknown as {
        modelContext?: { registerTool: (tool: unknown, options: unknown) => unknown };
      }
    ).modelContext;
    if (!context?.registerTool) return;
    const life = new AbortController();
    try {
      Promise.resolve(
        context.registerTool(
          {
            name: 'analyse_chess_profile',
            title: 'Analyse a Chess.com profile',
            description:
              'Fetch completed games for a username, show the overview, and start Stockfish checks on the latest five games.',
            inputSchema: {
              type: 'object',
              properties: {
                username: { type: 'string' },
                pace: { type: 'string', enum: ['rapid', 'blitz', 'bullet'] },
              },
              required: ['username'],
              additionalProperties: false,
            },
            annotations: { readOnlyHint: false, untrustedContentHint: true },
            execute: async (input: unknown) => {
              const args = input as { username?: unknown; pace?: unknown };
              if (
                !args ||
                typeof args.username !== 'string' ||
                (args.pace !== undefined &&
                  (typeof args.pace !== 'string' ||
                    !['rapid', 'blitz', 'bullet'].includes(args.pace)))
              )
                throw new Error('Provide a valid username and pace.');
              const result = await connect(args.username, (args.pace ?? 'rapid') as Pace);
              if (!result)
                throw new Error(
                  'The profile could not be loaded. See the visible error.',
                );
              await new Promise<void>((resolve) =>
                requestAnimationFrame(() => resolve()),
              );
              return { ...result, moveAnalysis: 'started' };
            },
          },
          { signal: life.signal },
        ),
      ).catch(() => {});
    } catch {}
    return () => life.abort();
  }, [connect]);
  function reset() {
    generation.current++;
    request.current?.abort();
    currentJob.current = null;
    setJobId(null);
    window.history.replaceState({}, '', '/');
    setData(null);
    setBusy(false);
    setEngineBusy(false);
    setError('');
  }
  function review(game: Game, ply?: number) {
    setSelected(game.id);
    setSelectedPly(ply);
    setView('review');
  }
  const checked = games.filter((g) => analyses[g.id]),
    findings = checked.flatMap((g) =>
      analyses[g.id].findings.map((f) => ({ game: g, f })),
    ),
    priority = findings
      .slice()
      .sort((a, b) => (b.f.loss ?? 100000) - (a.f.loss ?? 100000))[0];
  const selectedGame = games.find((g) => g.id === selected) ?? games[0];
  const graph = games.toReversed().map((g, i) => ({
    index: i + 1,
    rating: g.rating,
    date: dateLabel(g.date),
    opponent: g.opponent,
  }));
  if (!data)
    return (
      <Welcome
        onConnect={(name) => void connect(name)}
        onExample={() => void loadExample()}
        busy={busy}
        error={error}
        savedReviews={savedReviews}
        onResume={resumeJob}
      />
    );
  return (
    <div className="app-shell dashboard-shell">
      <header className="topbar dashboard-topbar">
        <a
          className="brand"
          href="/"
          onClick={(e) => {
            e.preventDefault();
            reset();
          }}
        >
          <span className="brand-mark">
            <ChessKnight size={25} />
          </span>
          tempo<span className="brand-dot">.</span>
        </a>
        <nav className="main-nav" aria-label="Review navigation">
          <Button
            variant="ghost"
            className={view === 'overview' ? 'active' : ''}
            onClick={() => setView('overview')}
          >
            <ChartNoAxesCombined size={16} /> Overview
          </Button>
          <Button
            variant="ghost"
            className={view === 'review' ? 'active' : ''}
            onClick={() => setView('review')}
            disabled={!games.length}
          >
            <ChessKnight size={16} /> Game review
          </Button>
        </nav>
        <Button variant="ghost" className="account-button" onClick={reset}>
          <span className="mini-avatar peach">
            {data.profile.username[0].toUpperCase()}
          </span>
          <span>{data.profile.username}</span>
          <span className="account-change">Change</span>
        </Button>
      </header>
      <main className="dashboard">
        <div className={`dashboard-heading ${view === 'review' ? 'compact' : ''}`}>
          <div>
            {view === 'overview' ? (
              <>
                <div className="eyebrow">
                  <span className="live-dot" />
                  {example ? 'Example review' : 'Reviewing ' + data.profile.username}
                </div>
                <h1>A little reflection. A better next move.</h1>
                <p>
                  {games.length} completed, rated {pace} games
                  {games.length > 0
                    ? `, ${dateLabel(games.at(-1)!.date)} to ${dateLabel(games[0].date)}`
                    : ''}
                </p>
              </>
            ) : (
              <p>Pick a game, replay it, and see what the engine found.</p>
            )}
          </div>
          <div className="heading-actions">
            <div className="pace-switch" aria-label="Time format">
              {(['rapid', 'blitz', 'bullet'] as Pace[]).map((p) => (
                <Button
                  key={p}
                  variant="ghost"
                  aria-pressed={pace === p}
                  className={pace === p ? 'active' : ''}
                  disabled={busy}
                  onClick={() => void connect(data.profile.username, p)}
                >
                  {p}
                </Button>
              ))}
            </div>
            <Button
              variant="outline"
              className="refresh-button"
              aria-label="Refresh games"
              onClick={() => void connect(data.profile.username, pace)}
              disabled={busy}
            >
              <RefreshCw size={17} className={busy ? 'spin' : ''} />
            </Button>
          </div>
        </div>
        {example && (
          <div className="notice">
            <span>You’re exploring 12 real games and their saved Stockfish review.</span>
            <Button variant="ghost" onClick={() => void connect(data.profile.username)}>
              Pull the latest games <ArrowRight size={15} />
            </Button>
          </div>
        )}
        {busy && (
          <div className="notice" role="status">
            <span>
              <LoaderCircle size={16} className="spin" /> Fetching your public profile and
              recent archives…
            </span>
          </div>
        )}
        {error && (
          <div className="error-message" role="alert">
            {error}
          </div>
        )}
        {data.warning && <p className="data-note">{data.warning}</p>}
        {data.games.length > games.length && (
          <p className="data-note">
            {data.games.length - games.length} games could not be safely replayed and are
            excluded.
          </p>
        )}
        {engineError && (
          <div className="error-message" role="alert">
            {engineError}{' '}
            {jobId && (
              <Button variant="ghost" onClick={() => resumeJob(jobId)}>
                Reconnect to saved review
              </Button>
            )}
            <Button
              variant="ghost"
              onClick={() =>
                void runAnalysis(games.filter((g) => !analyses[g.id]).slice(0, 5))
              }
            >
              Retry analysis
            </Button>
          </div>
        )}
        {!games.length ? (
          <div className="empty-state">
            <ChessKnight size={42} />
            <h2>No recent rated {pace} games found.</h2>
            <p>Try another time format, or come back after your next completed game.</p>
          </div>
        ) : view === 'review' ? (
          <GameReview
            key={`${selectedGame.id}:${analyses[selectedGame.id] ? 'checked' : 'pending'}:${selectedPly ?? ''}`}
            game={selectedGame}
            games={games}
            analyses={analyses}
            onSelect={(id) => {
              setSelected(id);
              setSelectedPly(undefined);
            }}
            focusPly={selectedPly}
            analysis={analyses[selectedGame.id]}
            busy={engineBusy}
            progress={engineProgress}
            onAnalyse={() => void runAnalysis([selectedGame])}
          />
        ) : (
          <>
            <section className="focus-panel">
              <button
                className="focus-board"
                onClick={() => review(priority?.game ?? games[0], priority?.f.ply)}
                aria-label="Open the featured position in game review"
              >
                <div className="spotlight-label">
                  <span className="live-dot" />
                  {priority ? 'A moment to revisit' : 'Your latest game'}
                  <ArrowUpRight size={16} />
                </div>
                <Board
                  fen={priority?.f.beforeFen ?? games[0].moves.at(-1)!.after}
                  flipped={(priority?.game ?? games[0]).colour === 'b'}
                />
                <div className="spotlight-caption">
                  <span>vs {(priority?.game ?? games[0]).opponent}</span>
                  <span>
                    {priority
                      ? `Move ${Math.ceil(priority.f.ply / 2)} · Your turn`
                      : dateLabel(games[0].date)}
                  </span>
                </div>
              </button>
              <div className="focus-copy">
                <div className="section-heading">
                  <span className="eyebrow">
                    <Target size={15} /> Your next focus
                  </span>
                </div>
                <h2>
                  {priority
                    ? priority.f.title
                    : stats.timeouts >= 2
                      ? 'Keep an eye on your clock.'
                      : engineBusy
                        ? 'Your next insight is on its way.'
                        : 'Build on what you’ve learned.'}
                </h2>
                <p>
                  {priority
                    ? `${findings.length} key moments across ${checked.length} checked games. Start with a position where the engine found a stronger move.`
                    : stats.timeouts >= 2
                      ? `${stats.timeouts} of your ${games.length} games ended in a loss on time. Practise checking your remaining time after each opponent move.`
                      : engineBusy
                        ? 'Stockfish is checking your moves. Your coaching priorities will appear here as each game finishes.'
                        : 'Review one game slowly. Look at checks, captures and threats before comparing your choice with the engine.'}
                </p>
                {priority ? (
                  <div className="focus-example">
                    <span>
                      Move {Math.ceil(priority.f.ply / 2)} against{' '}
                      {priority.game.opponent}
                    </span>
                    <strong>
                      {priority.f.actual}
                      <ArrowRight size={18} />
                      <span className="better">{priority.f.best}</span>
                    </strong>
                    <span>{priority.f.title}</span>
                  </div>
                ) : (
                  <div className="focus-example">
                    <ChessKnight size={32} />
                    <span>A useful review starts with a real position.</span>
                  </div>
                )}
                <Button
                  className="focus-button"
                  onClick={() => review(priority?.game ?? games[0], priority?.f.ply)}
                >
                  {priority ? 'Explore this moment' : 'Review your latest game'}
                  <ArrowRight size={17} />
                </Button>
                <div className="engine-status" role="status">
                  {engineBusy ? (
                    <LoaderCircle size={13} className="spin" />
                  ) : (
                    <Check size={13} />
                  )}
                  <span>
                    {engineBusy
                      ? engineProgress
                      : `${checked.length} of ${games.length} games checked`}
                  </span>
                  {engineBusy && (
                    <button
                      className="cancel-engine"
                      aria-label="Stop analysis and save completed games"
                      onClick={() => void cancelAnalysis()}
                    >
                      <X size={14} />
                    </button>
                  )}
                </div>
                {!engineBusy && checked.length < Math.min(5, games.length) && (
                  <Button
                    variant="link"
                    className="resume-analysis"
                    onClick={() =>
                      void runAnalysis(
                        games.filter((g) => !analyses[g.id]).slice(0, 5 - checked.length),
                      )
                    }
                  >
                    Continue analysis
                  </Button>
                )}
              </div>
            </section>
            <section className="metrics" aria-label="Your game statistics">
              <div className="metric">
                <div className="metric-label">
                  Latest recorded rating <TrendingUp size={16} />
                </div>
                <div className="metric-value">
                  {stats.rating?.toLocaleString()}
                  <span
                    className={`metric-change ${(stats.change ?? 0) >= 0 ? 'positive' : 'negative'}`}
                  >
                    {stats.change !== null ? (
                      <>
                        {stats.change >= 0 ? (
                          <ArrowUpRight size={15} />
                        ) : (
                          <ArrowDownRight size={15} />
                        )}{' '}
                        {stats.change >= 0 ? '+' : ''}
                        {stats.change}
                      </>
                    ) : (
                      '—'
                    )}
                  </span>
                </div>
                <p>Rating recorded in your latest game</p>
              </div>
              <div className="metric">
                <div className="metric-label">
                  Win rate <Target size={16} />
                </div>
                <div className="metric-value">
                  {stats.winRate}
                  <span className="value-unit">%</span>
                </div>
                <p className="metric-results">
                  <span>{stats.wins} wins</span>
                  <span>{stats.draw} draws</span>
                  <span>{stats.loss} losses</span>
                </p>
              </div>
              <div className="metric">
                <div className="metric-label">
                  Games imported <Layers size={16} />
                </div>
                <div className="metric-value">
                  {games.length}
                  <span className="value-unit">games</span>
                </div>
                <p>{checked.length} checked with Stockfish</p>
              </div>
              <div className="metric">
                <div className="metric-label">
                  Playing both sides <ChessKnight size={16} />
                </div>
                <div className="colour-results">
                  {stats.colours.map((c) => (
                    <div key={c.colour}>
                      <span
                        className={`colour-dot ${c.colour === 'w' ? 'white-dot' : 'black-dot'}`}
                      />
                      <strong>
                        {c.games ? `${Math.round((c.wins / c.games) * 100)}%` : '—'}
                      </strong>
                      <small>
                        {c.colour === 'w' ? 'White' : 'Black'} · {c.games} games
                      </small>
                    </div>
                  ))}
                </div>
                <p>Win rate by piece colour</p>
              </div>
            </section>
            <div className="overview-grid">
              <RatingPanelBoundary>
                <Suspense
                  fallback={
                    <section className="panel rating-panel" role="status">
                      Loading your rating trend…
                    </section>
                  }
                >
                  <RatingPanel graph={graph} pace={pace} change={stats.change} />
                </Suspense>
              </RatingPanelBoundary>
              <section className="panel games-panel">
                <div className="section-heading">
                  <h2>Your recent games</h2>
                  <span className="subtle-chip chip-pace">
                    <Clock3 size={13} /> {pace}
                  </span>
                </div>
                <div className="game-table-wrap">
                  <table className="game-table">
                    <thead>
                      <tr>
                        <th>Opponent</th>
                        <th>Result</th>
                        <th>Opening</th>
                        <th>Played</th>
                        <th>
                          <span className="sr-only">Review</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {games.slice(0, showAll ? games.length : 6).map((g) => (
                        <tr key={g.id}>
                          <td>
                            <button
                              className="opponent-button"
                              aria-label={`Review game against ${g.opponent}`}
                              onClick={() => review(g)}
                            >
                              <span
                                className={`colour-dot ${g.colour === 'w' ? 'white-dot' : 'black-dot'}`}
                                aria-hidden="true"
                              />
                              <span>
                                <strong>{g.opponent}</strong>
                                <small>
                                  {g.opponentRating} · {timeLabel(g.timeControl)}
                                </small>
                              </span>
                            </button>
                          </td>
                          <td>
                            <span className={`result-badge ${g.result}`}>
                              {g.result === 'win'
                                ? 'Won'
                                : g.result === 'loss'
                                  ? 'Lost'
                                  : 'Draw'}
                            </span>
                          </td>
                          <td className="table-opening">
                            <span>{g.opening}</span>
                            <small>
                              {analyses[g.id]
                                ? `${analyses[g.id].findings.length} key moments`
                                : 'Ready to review'}
                            </small>
                          </td>
                          <td className="table-date">{dateLabel(g.date)}</td>
                          <td>
                            <Button
                              variant="ghost"
                              size="icon"
                              aria-label={`Review game against ${g.opponent}`}
                              onClick={() => review(g)}
                            >
                              <ArrowUpRight size={18} />
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {games.length > 6 && (
                  <Button
                    variant="ghost"
                    className="view-all"
                    onClick={() => setShowAll(!showAll)}
                  >
                    {showAll ? 'Show fewer games' : `See all ${games.length} games`}
                    <ArrowRight size={15} />
                  </Button>
                )}
              </section>
              <section className="panel openings-panel">
                <div className="section-heading">
                  <h2>Opening repertoire</h2>
                  <ChessKnight size={19} />
                </div>
                <p className="panel-subtitle">
                  Your most played openings in this review.
                </p>
                <div className="openings-list">
                  {stats.openings.slice(0, 5).map((o) => {
                    const losses = o.games - o.wins - o.draws;
                    // A bar drawn from one or two games is dimmed rather than
                    // drawn at full strength, so a single loss cannot look like
                    // a collapsing repertoire.
                    const parts = [o.wins, o.draws, losses];
                    return (
                      <div className="opening-row" key={`${o.colour}-${o.name}`}>
                        <div className="opening-row-top">
                          <div>
                            <strong>{o.name}</strong>
                            <span>
                              {o.colour === 'w' ? 'As White' : 'As Black'} · {o.games}{' '}
                              {o.games === 1 ? 'game' : 'games'}
                            </span>
                          </div>
                          <b>
                            {Math.round((o.wins / o.games) * 100)}
                            <small>%</small>
                          </b>
                        </div>
                        <div
                          className={`result-bar ${o.games < 3 ? 'thin' : ''}`}
                          role="img"
                          aria-label={`${o.wins} won, ${o.draws} drawn, ${losses} lost, from ${o.games} ${o.games === 1 ? 'game' : 'games'}`}
                        >
                          {parts.map((count, i) =>
                            count > 0 ? (
                              <span
                                key={i}
                                className={['win', 'draw', 'loss'][i]}
                                style={{ width: `${(count / o.games) * 100}%` }}
                              />
                            ) : null,
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
                <div className="bar-legend">
                  <span>
                    <i /> Wins
                  </span>
                  <span>
                    <i /> Draws
                  </span>
                  <span>
                    <i /> Losses
                  </span>
                </div>
                <p className="small-sample-note">
                  Small samples are clues, not verdicts.
                </p>
              </section>
            </div>
          </>
        )}
        <details className="review-method">
          <summary>
            <CircleHelp size={15} /> About this review
          </summary>
          <div>
            <p>
              Trends use up to 40 completed, rated standard games from the eight latest
              active months. Chess.com may take up to 12 hours to refresh its public data.
              Ratings are those recorded in each game, not live account ratings.
            </p>
            <p>
              Automatic move review covers your latest five games. Native Stockfish 18
              evaluates positions on your server, and the original Rust analysis pipeline
              verifies completed games, selects key moments and checks tactical patterns.
              Reviews keep running when you close this tab and resume after a server
              restart.
            </p>
            <p>
              Each position uses 150,000 scan nodes and 1,000,000 confirmation nodes. Up
              to three drops of at least two pawns, or changes to forced mate, are shown
              per game. This search can miss mistakes. Evaluations are from your side;
              history-dependent repetition is not inferred. The example is a saved native
              Stockfish review of real completed games.
            </p>
            <p>
              <a
                href="https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it"
                target="_blank"
                rel="noreferrer"
              >
                Chess.com public data
              </a>{' '}
              ·{' '}
              <a
                href="https://github.com/official-stockfish/Stockfish/blob/sf_18/Copying.txt"
                target="_blank"
                rel="noreferrer"
              >
                Stockfish GPLv3 licence
              </a>{' '}
              ·{' '}
              <a
                href="https://github.com/official-stockfish/Stockfish/tree/sf_18"
                target="_blank"
                rel="noreferrer"
              >
                Engine source
              </a>
            </p>
          </div>
        </details>
      </main>
      <footer className="site-footer">
        <span>Made for the player you’re becoming.</span>
        <span>Independent of Chess.com.</span>
      </footer>
    </div>
  );
}
