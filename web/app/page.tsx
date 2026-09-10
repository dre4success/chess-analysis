import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  BookOpen,
  ChartNoAxesCombined,
  Check,
  ChessKnight,
  CircleHelp,
  LoaderCircle,
  RefreshCw,
  X,
} from 'lucide-react';
import Welcome from '../components/welcome';
import GameReview from '../components/game-review';
import ReviewLibrary from '../components/review-library';
import StudioOverview from '../components/studio-overview';
import { useReviewJob } from '../hooks/use-review-job';
import { useProfileHistory } from '../hooks/use-profile-history';
import {
  mappedAnalyses,
  remainingReviewGames,
  running,
  searchedReviews,
  type ReviewSummary,
} from '../lib/api';
import {
  normaliseUsername,
  parseGame,
  type Game,
  type GameAnalysis,
  type Pace,
  type PlayerData,
} from '../lib/chess';
import { convertFinding, type SavedReview } from '../lib/review-data';
import { gameKey, readRoute, routeUrl, type StudioRoute } from '../lib/navigation';
import type { StudyPosition } from '../lib/study';

type Example = { data: PlayerData; review: SavedReview };
export default function Home() {
  const [route, setRoute] = useState<StudioRoute>(() =>
    readRoute(window.location.search),
  );
  const [example, setExample] = useState<Example | null>(null);
  const [exampleBusy, setExampleBusy] = useState(false);
  const [localError, setLocalError] = useState('');
  const [saved, setSaved] = useState<ReviewSummary[]>([]);
  const { profiles, remember } = useProfileHistory();
  const historyScope = profiles.join(',');
  const source = useRef('');
  const exampleRequest = useRef<AbortController | null>(null);
  const scrollTarget = useRef<number | null>(null);
  const pendingFocus = useRef(false);
  const {
    job,
    display,
    submitting,
    error,
    disconnected,
    start,
    resume,
    cancel,
    disconnect,
  } = useReviewJob();
  const data = example?.data ?? display?.data;
  const busy = submitting || exampleBusy;
  const hasData = !!data;
  const active = !!job && running(job) && !disconnected;
  const reviewData = example?.review ?? display?.review;
  const games = useMemo(
    () =>
      data
        ? data.games
            .map((g) => parseGame(g, data.profile.username.toLowerCase()))
            .filter((g): g is Game => !!g)
        : [],
    [data],
  );
  const analyses = useMemo<Record<string, GameAnalysis>>(
    () =>
      example
        ? Object.fromEntries(
            example.review.games.map((g) => [
              g.url,
              {
                id: g.url,
                source: 'verified' as const,
                positions: 0,
                findings: g.findings.map(convertFinding),
              },
            ]),
          )
        : display
          ? mappedAnalyses(display)
          : {},
    [example, display],
  );
  const pace = data?.pace ?? 'rapid';
  const remaining = remainingReviewGames(job, games, analyses);
  const selected = games.find((g) => gameKey(g.id) === route.gameId) ?? games[0];
  const unknownGame =
    !!route.gameId && !games.some((g) => gameKey(g.id) === route.gameId);

  const navigate = useCallback(
    (next: StudioRoute, replace = false, restoreTop = true) => {
      if (!replace)
        window.history.replaceState(
          { ...window.history.state, scrollY: window.scrollY },
          '',
          window.location.href,
        );
      window.history[replace ? 'replaceState' : 'pushState'](
        { scrollY: restoreTop ? 0 : window.scrollY },
        '',
        routeUrl(next),
      );
      if (restoreTop) {
        scrollTarget.current = 0;
        pendingFocus.current = true;
      }
      setRoute(next);
    },
    [],
  );
  useEffect(() => {
    const pop = () => {
      scrollTarget.current = window.history.state?.scrollY ?? 0;
      pendingFocus.current = true;
      setRoute(readRoute(window.location.search));
    };
    window.addEventListener('popstate', pop);
    return () => window.removeEventListener('popstate', pop);
  }, []);
  useEffect(() => {
    if (scrollTarget.current === null) return;
    const frame = requestAnimationFrame(() => {
      window.scrollTo({ top: scrollTarget.current ?? 0, behavior: 'instant' });
      scrollTarget.current = null;
      if (pendingFocus.current) {
        const target = document.querySelector<HTMLElement>(
          route.view === 'review' ? '.board-keyboard' : 'main h1, main h2',
        );
        if (target) {
          target.setAttribute('tabindex', '-1');
          if (route.view === 'review') target.setAttribute('tabindex', '0');
          target.focus({ preventScroll: true });
          pendingFocus.current = false;
        }
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [route.view, route.gameId, hasData, exampleBusy]);

  useEffect(() => {
    if (route.reviewId) {
      const key = `review:${route.reviewId}`;
      if (source.current === key) return;
      source.current = key;
      exampleRequest.current?.abort();
      setExample(null);
      setExampleBusy(false);
      resume(route.reviewId);
    } else if (route.example && source.current !== 'example') {
      source.current = 'example';
      disconnect(true);
      const c = new AbortController();
      exampleRequest.current?.abort();
      exampleRequest.current = c;
      setExampleBusy(true);
      setLocalError('');
      Promise.all([
        fetch('/example/games.json', { signal: c.signal }).then((r) => {
          if (!r.ok) throw new Error('Could not load the example games.');
          return r.json() as Promise<{ games: PlayerData['games'] }>;
        }),
        fetch('/example/review.json', { signal: c.signal }).then((r) => {
          if (!r.ok) throw new Error('Could not load the example review.');
          return r.json() as Promise<SavedReview>;
        }),
      ])
        .then(([g, r]) => {
          if (c.signal.aborted) return;
          setExample({
            review: r,
            data: {
              profile: { username: r.user },
              games: g.games.toSorted((a, b) => b.end_time - a.end_time),
              pace: 'rapid',
              fetchedAt: r.generated,
              archivesRead: 1,
            },
          });
        })
        .catch((e) => {
          if (!c.signal.aborted) {
            source.current = '';
            setLocalError(e.message);
            navigate({ view: 'welcome' }, true);
          }
        })
        .finally(() => {
          if (!c.signal.aborted) setExampleBusy(false);
        });
    } else if (!route.example && source.current) {
      source.current = '';
      exampleRequest.current?.abort();
      disconnect(true);
      setExample(null);
      setExampleBusy(false);
    }
  }, [route.reviewId, route.example, resume, disconnect, navigate]);
  useEffect(() => () => exampleRequest.current?.abort(), []);
  useEffect(() => {
    if (route.view !== 'welcome') return;
    const c = new AbortController();
    searchedReviews(profiles, { limit: 4, signal: c.signal })
      .then((p) => {
        if (!c.signal.aborted) setSaved(p.reviews);
      })
      .catch(() => {});
    return () => c.abort();
  }, [route.view, profiles]);

  const connect = useCallback(
    async (name: string, nextPace: Pace = 'rapid') => {
      try {
        const username = normaliseUsername(name);
        setLocalError('');
        exampleRequest.current?.abort();
        setExampleBusy(false);
        const next = await start({ username, pace: nextPace });
        if (!next) return null;
        remember(next.username);
        source.current = `review:${next.id}`;
        setExample(null);
        navigate({ view: 'overview', reviewId: next.id });
        return { username: next.username, reviewId: next.id, pace: next.pace };
      } catch (e) {
        setLocalError((e as Error).message);
        return null;
      }
    },
    [start, navigate, remember],
  );
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
            description: 'Import completed games and start a saved native review.',
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
                throw new Error('Provide a username and supported pace.');
              const result = await connect(args.username, (args.pace ?? 'rapid') as Pace);
              if (!result)
                throw new Error('The review could not start. See the visible error.');
              return { ...result, moveAnalysis: 'started' };
            },
          },
          { signal: life.signal },
        ),
      ).catch(() => {});
    } catch {}
    return () => life.abort();
  }, [connect]);
  function home() {
    source.current = '';
    exampleRequest.current?.abort();
    disconnect(true);
    setExample(null);
    setLocalError('');
    setExampleBusy(false);
    navigate({ view: 'welcome' });
  }
  function openReview(id: number) {
    setLocalError('');
    navigate({ view: 'overview', reviewId: id });
  }
  function library() {
    navigate({ view: 'library', reviewId: route.reviewId, example: route.example });
  }
  function inspect(game: Game, ply?: number) {
    navigate({
      view: 'review',
      reviewId: route.reviewId,
      example: route.example,
      gameId: gameKey(game.id),
      position: ply ? { ply: ply - 1, mode: 'before', findingPly: ply } : undefined,
    });
  }
  function move(position: StudyPosition) {
    navigate({ ...route, gameId: gameKey(selected.id), position }, true, false);
  }
  async function analyse(batch: Game[]) {
    if (!data || !batch.length) return;
    if (!job || example) {
      await connect(data.profile.username, pace);
      return;
    }
    const next = await start({
      username: data.profile.username,
      pace,
      parent_id: job.id,
      game_urls: batch.map((g) => g.id),
    });
    if (next) {
      remember(next.username);
      source.current = `review:${next.id}`;
      navigate({ ...route, reviewId: next.id, example: undefined }, true, false);
    }
  }
  const problem = localError || error;
  if (route.view === 'welcome' || (!data && route.view !== 'library'))
    return (
      <Welcome
        onConnect={(n) => void connect(n)}
        onExample={() => navigate({ view: 'overview', example: true })}
        busy={busy}
        error={problem}
        savedReviews={saved.filter((review) =>
          profiles.includes(review.username.toLowerCase()),
        )}
        onResume={openReview}
        onLibrary={library}
      />
    );
  return (
    <div
      className={`app-shell studio-shell ${route.view === 'review' ? 'study-shell' : ''}`}
    >
      <header className="topbar dashboard-topbar">
        <a
          href="/"
          className="brand"
          aria-label="Tempo home"
          onClick={(e) => {
            e.preventDefault();
            home();
          }}
        >
          <span className="brand-mark">
            <ChessKnight size={26} />
          </span>
          tempo<span className="brand-period">.</span>
        </a>
        <nav className="main-nav" aria-label="Studio navigation">
          <button
            aria-current={route.view === 'overview' ? 'page' : undefined}
            className={route.view === 'overview' ? 'active' : ''}
            disabled={!data}
            onClick={() =>
              navigate({
                view: 'overview',
                reviewId: route.reviewId,
                example: route.example,
              })
            }
          >
            <ChartNoAxesCombined size={17} /> Overview
          </button>
          <button
            aria-current={route.view === 'review' ? 'page' : undefined}
            className={route.view === 'review' ? 'active' : ''}
            disabled={!games.length}
            onClick={() => inspect(selected)}
          >
            <ChessKnight size={17} /> Study room
          </button>
          <button
            aria-current={route.view === 'library' ? 'page' : undefined}
            className={route.view === 'library' ? 'active' : ''}
            onClick={library}
          >
            <BookOpen size={17} /> Library
          </button>
        </nav>
        <button className="account-button" onClick={home}>
          <span className="mini-avatar">
            {data?.profile.username[0].toUpperCase() ?? 'T'}
          </span>
          <span>{data?.profile.username ?? 'Your studio'}</span>
          <ArrowUpRight size={16} />
        </button>
      </header>
      <main className={`dashboard ${route.view === 'review' ? 'study-dashboard' : ''}`}>
        {route.view === 'library' ? (
          <ReviewLibrary
            key={historyScope}
            profiles={profiles}
            onResume={openReview}
            onSearch={home}
          />
        ) : (
          <>
            <div
              className={`dashboard-heading ${route.view === 'review' ? 'compact' : ''}`}
            >
              <div>
                {route.view === 'overview' ? (
                  <>
                    <span className="eyebrow">
                      <span className="live-dot" />
                      {example
                        ? 'A REAL REVIEW, READY TO EXPLORE'
                        : 'YOUR PERSONAL CHESS STUDIO'}
                    </span>
                    <h1>
                      Your chess,
                      <br className="mobile-break" /> <em>in perspective.</em>
                    </h1>
                    <p>A little reflection today. A more thoughtful move tomorrow.</p>
                  </>
                ) : (
                  <>
                    <button
                      className="back-link"
                      onClick={() =>
                        navigate({
                          view: 'overview',
                          reviewId: route.reviewId,
                          example: route.example,
                        })
                      }
                    >
                      <ArrowLeft size={16} /> Back to overview
                    </button>
                    <h1>
                      The study room<span className="heading-dot">.</span>
                    </h1>
                  </>
                )}
              </div>
              <div className="heading-actions">
                <div className="pace-switch" aria-label="Time format">
                  {(['rapid', 'blitz', 'bullet'] as Pace[]).map((p) => (
                    <button
                      key={p}
                      aria-pressed={pace === p}
                      disabled={busy}
                      onClick={() => void connect(data!.profile.username, p)}
                    >
                      {p}
                    </button>
                  ))}
                </div>
                <button
                  className="icon-button"
                  title="Import the latest games"
                  aria-label="Refresh games"
                  disabled={busy}
                  onClick={() => void connect(data!.profile.username, pace)}
                >
                  <RefreshCw size={18} className={busy ? 'spin' : ''} />
                </button>
              </div>
            </div>
            {example && (
              <div className="example-notice">
                <span>
                  <SparkIcon /> You’re exploring {games.length} completed games with a
                  saved review.
                </span>
                <button onClick={() => void connect(data!.profile.username, pace)}>
                  Import your latest games <ArrowRight size={15} />
                </button>
              </div>
            )}
            {job && (active || job.status === 'cancelled' || job.status === 'failed') && (
              <div
                className={`job-banner ${job.status === 'failed' ? 'job-failed' : ''}`}
                role="status"
              >
                <span className="job-icon">
                  {active ? (
                    <LoaderCircle size={19} className="spin" />
                  ) : (
                    <Check size={19} />
                  )}
                </span>
                <div>
                  <strong>
                    {job.status === 'queued'
                      ? 'Your review is in the queue'
                      : job.status === 'cancelled'
                        ? 'Review stopped. Your finished games are saved.'
                        : job.status === 'failed'
                          ? 'This review needs another try.'
                          : job.message}
                  </strong>
                  <span>
                    {job.progress.completed} of{' '}
                    {job.progress.total ||
                      job.target_urls?.length ||
                      Math.min(5, games.length) ||
                      5}{' '}
                    games saved in this batch
                    {active ? ' · You can keep exploring while we work.' : ''}
                  </span>
                </div>
                {active && (
                  <button
                    className="text-button"
                    onClick={() => void cancel()}
                    aria-label="Stop analysis and save completed games"
                  >
                    <X size={16} /> Stop
                  </button>
                )}
                {['cancelled', 'failed'].includes(job.status) &&
                  !busy &&
                  remaining.length > 0 && (
                    <button
                      className="text-button"
                      onClick={() => void analyse(remaining)}
                    >
                      Continue <ArrowRight size={16} />
                    </button>
                  )}
              </div>
            )}
            {busy && (
              <p className="loading-line" role="status">
                <LoaderCircle size={17} className="spin" /> Finding your completed games…
              </p>
            )}
            {problem && (
              <div className="error-message" role="alert">
                {problem}{' '}
                {job && (
                  <button onClick={() => resume(job.id)}>
                    Reconnect to saved review
                  </button>
                )}
              </div>
            )}
            {data?.warning && <p className="data-note">{data.warning}</p>}
            {data && data.games.length > games.length && (
              <p className="data-note">
                {data.games.length - games.length} games could not be safely replayed and
                were excluded.
              </p>
            )}
            {!games.length ? (
              <div className="empty-state">
                <ChessKnight size={40} />
                <h2>No recent rated {pace} games found.</h2>
                <p>
                  Choose another time format or return after your next completed game.
                </p>
              </div>
            ) : route.view === 'review' && unknownGame ? (
              <div className="empty-state" role="status">
                <BookOpen size={36} />
                <h2>This game isn’t in this review.</h2>
                <p>
                  The saved position refers to a different game. Choose one from this
                  review to continue.
                </p>
                <button
                  className="ink-button"
                  onClick={() =>
                    navigate({
                      view: 'overview',
                      reviewId: route.reviewId,
                      example: route.example,
                    })
                  }
                >
                  Choose a game <ArrowRight size={17} />
                </button>
              </div>
            ) : route.view === 'review' ? (
              <GameReview
                key={selected.id}
                game={selected}
                games={games}
                analyses={analyses}
                analysis={analyses[selected.id]}
                onSelect={(id) => {
                  const g = games.find((g) => g.id === id);
                  if (g) inspect(g);
                }}
                position={route.position}
                onPositionChange={move}
                onAnalyse={() => void analyse([selected])}
                busy={active}
                progress={job?.message ?? ''}
              />
            ) : (
              <StudioOverview
                games={games}
                analyses={analyses}
                pace={pace}
                onReview={inspect}
                digestUrl={
                  !example && display?.review
                    ? `/api/reviews/${display.id}/digest`
                    : undefined
                }
                busy={active}
                canContinue={!example && remaining.length > 0}
                onContinue={() => void analyse(remaining)}
              />
            )}
            <details className="review-method">
              <summary>
                <CircleHelp size={16} /> How this review works
              </summary>
              <div>
                <p>
                  Trends use up to 40 completed, rated standard games from the latest
                  eight active months. Ratings are recorded in each game. Chess.com public
                  data can take time to update.
                </p>
                <p>
                  The latest five games are checked automatically. Each position uses
                  150,000 scan nodes and 1,000,000 confirmation nodes with native
                  Stockfish. Up to three substantial mistakes are selected per game. A
                  bounded search can miss mistakes; an empty review does not mean perfect
                  play.
                </p>
                <p>
                  New analysis includes the game’s move history. Older saved reviews
                  retain their original analysis. Engine verdicts and proposed tactical
                  explanations have different limits: compare the actual continuation with
                  the alternative and treat small pattern samples as observations.
                </p>
                <p>
                  Reviews stay saved on this server. Your library shows players you’ve
                  looked up in this browser. Closing a tab stops polling, and an explicit
                  Stop keeps completed results.{' '}
                  <a
                    href="https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it"
                    target="_blank"
                    rel="noreferrer"
                  >
                    Chess.com public data
                  </a>{' '}
                  ·{' '}
                  <a
                    href="https://github.com/official-stockfish/Stockfish/tree/sf_18"
                    target="_blank"
                    rel="noreferrer"
                  >
                    Stockfish source & licence
                  </a>
                </p>
                {reviewData?.generated && (
                  <p>
                    Review generated {new Date(reviewData.generated).toLocaleString()}.
                  </p>
                )}
              </div>
            </details>
          </>
        )}
      </main>
      <footer className="site-footer">
        <a
          className="brand"
          href="/"
          onClick={(e) => {
            e.preventDefault();
            home();
          }}
        >
          tempo.
        </a>
        <span>Make room for your next discovery.</span>
        <span>Independent of Chess.com.</span>
      </footer>
    </div>
  );
}
function SparkIcon() {
  return (
    <span aria-hidden="true" className="small-star">
      ✳
    </span>
  );
}
