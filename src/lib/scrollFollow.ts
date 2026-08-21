export const SCROLL_FOLLOW_THRESHOLD_PX = 72;
export const SCROLL_DIRECTION_EPSILON_PX = 1;

export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

export interface FollowTransition {
  following: boolean;
  previousScrollTop: number;
  current: ScrollMetrics;
  upwardIntent?: boolean;
  threshold?: number;
}

export function distanceFromBottom(metrics: ScrollMetrics): number {
  return Math.max(0, metrics.scrollHeight - metrics.clientHeight - metrics.scrollTop);
}

export function shouldFollowAfterScroll({
  following,
  previousScrollTop,
  current,
  upwardIntent = false,
  threshold = SCROLL_FOLLOW_THRESHOLD_PX,
}: FollowTransition): boolean {
  const movement = current.scrollTop - previousScrollTop;
  if (upwardIntent || movement < -SCROLL_DIRECTION_EPSILON_PX) return false;
  if (!following && movement > SCROLL_DIRECTION_EPSILON_PX && distanceFromBottom(current) <= threshold) return true;
  return following;
}

export function pinToBottom(viewport: ScrollMetrics): void {
  viewport.scrollTop = viewport.scrollHeight;
}
