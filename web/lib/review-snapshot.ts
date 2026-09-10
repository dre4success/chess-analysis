import type { ReviewJob } from './api.ts';

/** Live progress belongs to `job`; keep display identity until its content changes. */
export function stableReviewDisplay(
  previous: ReviewJob | null,
  next: ReviewJob,
  parentId?: number,
): ReviewJob | null {
  const samePlayer = previous?.username === next.username && previous.pace === next.pace;
  const sameReview = samePlayer && previous?.id === next.id;
  if (!next.data) {
    if (sameReview) return previous;
    // An explicitly requested extension owns a copy of this exact parent sample.
    if (samePlayer && previous?.id === parentId && previous.data)
      return { ...next, data: previous.data, review: previous.review };
    return null;
  }
  if (!sameReview || !previous) return next;
  const data =
    JSON.stringify(previous.data) === JSON.stringify(next.data)
      ? previous.data
      : next.data;
  const review =
    JSON.stringify(previous.review) === JSON.stringify(next.review)
      ? previous.review
      : next.review;
  if (data === previous.data && review === previous.review) return previous;
  return { ...next, data, review };
}
