import { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, BookOpen, Clock3, LoaderCircle, Search } from 'lucide-react';
import { searchedReviews, type ReviewSummary } from '../lib/api';
import { dateLabel } from '../lib/chess';
export default function ReviewLibrary({
  profiles,
  onResume,
  onSearch,
}: {
  profiles: string[];
  onResume: (id: number) => void;
  onSearch: () => void;
}) {
  const request = useRef<AbortController | null>(null);
  const [reviews, setReviews] = useState<ReviewSummary[]>([]),
    [total, setTotal] = useState(0),
    [hasMore, setHasMore] = useState(false),
    [busy, setBusy] = useState(true),
    [error, setError] = useState(''),
    [query, setQuery] = useState('');
  useEffect(() => {
    const c = new AbortController();
    request.current?.abort();
    request.current = c;
    searchedReviews(profiles, { limit: 20, signal: c.signal })
      .then((p) => {
        if (c.signal.aborted) return;
        setReviews(p.reviews);
        setTotal(p.total ?? p.reviews.length);
        setHasMore(p.has_more);
      })
      .catch((e) => {
        if (!c.signal.aborted) setError(e.message);
      })
      .finally(() => {
        if (!c.signal.aborted) setBusy(false);
      });
    return () => request.current?.abort();
  }, [profiles]);
  async function more() {
    request.current?.abort();
    const c = new AbortController();
    request.current = c;
    setBusy(true);
    setError('');
    try {
      const p = await searchedReviews(profiles, {
        offset: reviews.length,
        limit: 20,
        signal: c.signal,
      });
      if (c.signal.aborted) return;
      setReviews((old) => [
        ...old,
        ...p.reviews.filter((r) => !old.some((o) => o.id === r.id)),
      ]);
      setHasMore(p.has_more);
      setTotal(p.total);
    } catch (e) {
      if (!c.signal.aborted) setError((e as Error).message);
    } finally {
      if (!c.signal.aborted) setBusy(false);
    }
  }
  const filtered = reviews.filter((r) =>
    `${r.username} ${r.pace} ${r.id}`.toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <div className="library-content">
      <div className="dashboard-heading">
        <div>
          <span className="eyebrow">THE COLLECTION</span>
          <h1>
            Your chess, <em>kept.</em>
          </h1>
          <p>Reviews for players you’ve looked up in this browser.</p>
        </div>
        <span className="count-seal">
          <BookOpen size={22} />
          {total} saved
        </span>
      </div>
      {reviews.length > 0 && (
        <div className="library-toolbar">
          <label className="library-search">
            <Search size={18} />
            <input
              type="search"
              placeholder="Find a player, format or review number"
              aria-label="Search loaded reviews"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          <span>
            Showing {filtered.length} of {total} saved reviews
          </span>
        </div>
      )}
      {error && (
        <p className="error-message" role="alert">
          {error}
        </p>
      )}
      <div className="library-grid">
        {filtered.map((r, i) => (
          <button
            className={`library-card library-tone-${i % 3}`}
            key={r.id}
            onClick={() => onResume(r.id)}
          >
            <div className="library-card-top">
              <span className="eyebrow">
                {r.pace} / REVIEW {String(r.id).padStart(3, '0')}
              </span>
              <ArrowUpRight size={22} />
            </div>
            <div className="library-monogram">
              {r.username.slice(0, 1).toUpperCase()}
              <span>♞</span>
            </div>
            <h2>{r.username}</h2>
            <p>
              {r.first_game_at && r.last_game_at
                ? `${dateLabel(r.first_game_at)} – ${dateLabel(r.last_game_at)}`
                : new Date(r.created_at).toLocaleDateString('en-GB', {
                    day: 'numeric',
                    month: 'long',
                    year: 'numeric',
                  })}
            </p>
            <div className="library-card-stats">
              <span>
                <strong>{r.reviewed_games ?? r.progress.completed}</strong> checked
                {r.imported_games ? ` / ${r.imported_games} imported` : ''}
              </span>
              <span className={`status-chip status-${r.status}`}>
                {r.status === 'complete'
                  ? 'Ready to explore'
                  : r.status === 'cancelled'
                    ? 'Stopped · saved'
                    : r.status}
              </span>
            </div>
            <div className="library-card-foot">
              <Clock3 size={13} />
              <span>
                Saved{' '}
                {new Date(r.created_at).toLocaleString('en-GB', {
                  day: 'numeric',
                  month: 'short',
                  hour: '2-digit',
                  minute: '2-digit',
                })}
              </span>
            </div>
          </button>
        ))}
      </div>
      {!busy && !error && !reviews.length && (
        <div className="empty-state">
          <BookOpen size={38} />
          <h2>Your library starts with a search.</h2>
          <p>
            Look up a Chess.com username to find their games. This browser keeps its own
            list of players.
          </p>
          <button className="ink-button" onClick={onSearch}>
            Find a player <ArrowUpRight size={17} />
          </button>
        </div>
      )}
      {!busy && reviews.length > 0 && !filtered.length && (
        <p className="data-note">
          No matches in the loaded reviews.
          {hasMore ? ' Load more to search older reviews.' : ''}
        </p>
      )}
      {(hasMore || busy) && (
        <button
          className="secondary-button load-more"
          onClick={() => void more()}
          disabled={busy}
        >
          {busy ? (
            <>
              <LoaderCircle size={17} className="spin" /> Loading your library…
            </>
          ) : (
            'Load more reviews'
          )}
        </button>
      )}
    </div>
  );
}
