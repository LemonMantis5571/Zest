/** A turn must occupy the user's attention long enough to merit a completion notice. */
export const LONG_TURN_NOTIFICATION_MS = 10_000;

export function isLongTurn(durationMs: number) {
  return durationMs >= LONG_TURN_NOTIFICATION_MS;
}
