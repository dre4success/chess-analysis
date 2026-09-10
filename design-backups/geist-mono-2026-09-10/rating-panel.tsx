import { useId, useMemo, useState } from 'react';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ArrowUpRight, ArrowDownRight, TrendingUp } from 'lucide-react';
import type { Pace } from '../lib/chess';
export default function RatingPanel({
  graph,
  pace,
  change,
}: {
  graph: {
    index: number;
    rating: number;
    date: string;
    opponent: string;
    result?: string;
  }[];
  pace: Pace;
  change: number | null;
}) {
  const [range, setRange] = useState(40);
  const id = useId().replaceAll(':', '');
  const points = useMemo(() => graph.slice(-range), [graph, range]);
  const latest = points.at(-1)?.rating;
  const delta = points.length > 1 ? points.at(-1)!.rating - points[0].rating : null;
  const low = Math.min(...points.map((p) => p.rating));
  const high = Math.max(...points.map((p) => p.rating));
  const padding = Math.max(15, Math.ceil((high - low) * 0.2));
  return (
    <section className="panel rating-panel" aria-label="Rating journey">
      <div className="section-heading">
        <div>
          <span className="eyebrow">THE BIGGER PICTURE</span>
          <h2>Your rating journey</h2>
        </div>
        <TrendingUp className="panel-icon" size={23} />
      </div>
      <div className="rating-headline">
        <strong>{latest?.toLocaleString() ?? '—'}</strong>
        <span className={`trend-chip ${(delta ?? 0) >= 0 ? 'positive' : 'negative'}`}>
          {delta === null ? (
            'First recorded game'
          ) : (
            <>
              {delta >= 0 ? <ArrowUpRight size={15} /> : <ArrowDownRight size={15} />}{' '}
              {delta >= 0 ? '+' : ''}
              {delta} points
            </>
          )}
        </span>
        <span className="rating-pace">{pace}</span>
      </div>
      <div className="chart-toolbar">
        <p>The ups, the downs, the direction.</p>
        <div className="chart-range" aria-label="Chart range">
          {[10, 20, 40].map((n) => (
            <button key={n} aria-pressed={range === n} onClick={() => setRange(n)}>
              {n === 40 ? 'All' : `Last ${n}`}
            </button>
          ))}
        </div>
      </div>
      <div
        className="rating-chart"
        role="img"
        aria-label={
          points.length > 1
            ? `${points.length} recorded game ratings, from ${points[0].rating} to ${latest}. Change ${delta}.`
            : 'A rating trend needs at least two games.'
        }
      >
        {points.length > 1 ? (
          <ResponsiveContainer
            width="100%"
            height="100%"
            minWidth={0}
            initialDimension={{ width: 640, height: 250 }}
          >
            <AreaChart data={points} margin={{ top: 20, right: 12, bottom: 0, left: -8 }}>
              <defs>
                <linearGradient id={`rating-${id}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="#34796d" stopOpacity={0.28} />
                  <stop offset="100%" stopColor="#34796d" stopOpacity={0.015} />
                </linearGradient>
              </defs>
              <CartesianGrid vertical={false} stroke="#e7e3db" strokeDasharray="3 5" />
              <XAxis
                dataKey="index"
                axisLine={false}
                tickLine={false}
                minTickGap={76}
                tick={{ fontSize: 14, fill: '#655e69' }}
                tickFormatter={(i) => graph.find((p) => p.index === i)?.date ?? ''}
              />
              <YAxis
                domain={[low - padding, high + padding]}
                axisLine={false}
                tickLine={false}
                allowDecimals={false}
                tick={{ fontSize: 14, fill: '#655e69' }}
                tickCount={4}
              />
              <ReferenceLine
                y={points[0].rating}
                stroke="#b5aea3"
                strokeDasharray="4 5"
              />
              <Tooltip
                content={({ active, payload }) =>
                  active && payload?.[0] ? (
                    <div className="chart-tooltip">
                      <strong>
                        {payload[0].payload.rating} <small>rating</small>
                      </strong>
                      <span>
                        {payload[0].payload.date} · vs {payload[0].payload.opponent}
                      </span>
                      {payload[0].payload.result && (
                        <span className="tooltip-result">
                          {payload[0].payload.result}
                        </span>
                      )}
                    </div>
                  ) : null
                }
              />
              <Area
                type="linear"
                dataKey="rating"
                stroke="#34796d"
                strokeWidth={3}
                fill={`url(#rating-${id})`}
                activeDot={{ r: 6, fill: '#34796d', stroke: '#fff', strokeWidth: 3 }}
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        ) : (
          <div className="empty-chart">
            <TrendingUp size={32} />
            <p>Your next game starts the story.</p>
          </div>
        )}
      </div>
      <div className="chart-foot">
        <span>Ratings recorded in each game</span>
        <span>
          {points.length} games shown ·{' '}
          {change === null
            ? 'one rating'
            : `${change >= 0 ? '+' : ''}${change} in full sample`}
        </span>
      </div>
    </section>
  );
}
