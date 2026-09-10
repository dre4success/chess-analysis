import { useCallback, useEffect, useRef, useState } from 'react';
import {
  api,
  running,
  watchReview,
  type ReviewJob,
  type ReviewRequest,
} from '../lib/api';
import { stableReviewDisplay } from '../lib/review-snapshot';

/** Polling owns the connection; only an explicit cancel changes the server job. */
export function useReviewJob() {
  const [job, setJob] = useState<ReviewJob | null>(null);
  const [display, setDisplay] = useState<ReviewJob | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');
  const [disconnected, setDisconnected] = useState(false);
  const connection = useRef<AbortController | null>(null);
  const latestJob = useRef<ReviewJob | null>(null);
  const latestDisplay = useRef<ReviewJob | null>(null);
  const mounted = useRef(true);

  const clearResults = useCallback(() => {
    latestJob.current = null;
    latestDisplay.current = null;
    setJob(null);
    setDisplay(null);
  }, []);
  const publish = useCallback((next: ReviewJob, parentId?: number) => {
    latestJob.current = next;
    setJob(next);
    const usable = stableReviewDisplay(latestDisplay.current, next, parentId);
    if (usable !== latestDisplay.current) {
      latestDisplay.current = usable;
      setDisplay(usable);
    }
  }, []);
  const currentConnection = useCallback(
    (controller: AbortController) =>
      mounted.current && connection.current === controller && !controller.signal.aborted,
    [],
  );

  const disconnect = useCallback(
    (clear = false) => {
      connection.current?.abort();
      connection.current = null;
      setSubmitting(false);
      setDisconnected(!clear && !!latestJob.current && running(latestJob.current));
      setError('');
      if (clear) clearResults();
    },
    [clearResults],
  );
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      connection.current?.abort();
    };
  }, []);

  const follow = useCallback(
    async (id: number, controller: AbortController) => {
      try {
        const updates = watchReview(id, controller.signal);
        while (currentConnection(controller)) {
          const update = await updates.next();
          if (update.done) return;
          const next = update.value;
          if (!currentConnection(controller)) return;
          if (next.id !== id)
            throw new Error(
              'The server returned a different saved review. Reconnect to try again.',
            );
          publish(next);
          if (next.data || !running(next)) setSubmitting(false);
          if (next.status === 'failed') setError(next.error || next.message);
        }
      } catch (e) {
        if (currentConnection(controller)) {
          setSubmitting(false);
          setDisconnected(true);
          setError(e instanceof Error ? e.message : 'Could not reach your saved review.');
        }
      }
    },
    [currentConnection, publish],
  );

  const resume = useCallback(
    (id: number) => {
      disconnect();
      if (
        latestJob.current?.id !== id ||
        (latestDisplay.current && latestDisplay.current.id !== id)
      )
        clearResults();
      const controller = new AbortController();
      connection.current = controller;
      setSubmitting(true);
      setDisconnected(false);
      void follow(id, controller);
    },
    [clearResults, disconnect, follow],
  );

  const start = useCallback(
    async (input: ReviewRequest) => {
      disconnect();
      const controller = new AbortController();
      connection.current = controller;
      setSubmitting(true);
      try {
        const next = await api<ReviewJob>('/reviews', controller.signal, input);
        if (!currentConnection(controller)) return null;
        publish(next, input.parent_id);
        setDisconnected(false);
        if (next.data || !running(next)) setSubmitting(false);
        void follow(next.id, controller);
        return next;
      } catch (e) {
        if (currentConnection(controller)) {
          setSubmitting(false);
          setDisconnected(!!latestJob.current);
          setError(e instanceof Error ? e.message : 'Could not start your review.');
        }
        return null;
      }
    },
    [currentConnection, disconnect, follow, publish],
  );

  const cancel = useCallback(async () => {
    const target = latestJob.current;
    if (!target) return;
    const controller = connection.current;
    const stillCurrent = () =>
      mounted.current &&
      connection.current === controller &&
      !controller?.signal.aborted &&
      latestJob.current?.id === target.id;
    try {
      // Keep an explicit cancellation independent of subsequent client navigation.
      const next = await api<ReviewJob>(`/reviews/${target.id}/cancel`, undefined, {});
      if (!stillCurrent()) return;
      if (next.id !== target.id)
        throw new Error('The server returned a different saved review.');
      connection.current?.abort();
      connection.current = null;
      publish(next);
      setSubmitting(false);
      setDisconnected(false);
      setError('');
    } catch (e) {
      if (stillCurrent())
        setError(e instanceof Error ? e.message : 'Could not stop this review.');
    }
  }, [publish]);
  return {
    job,
    display,
    submitting,
    error,
    disconnected,
    start,
    resume,
    cancel,
    disconnect,
  };
}
