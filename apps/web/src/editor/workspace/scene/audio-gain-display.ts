/** Classic clip-line presentation, not audio evaluation. -60 dB is not mute. */
export function gainLinePercent(db: number) {
  const gain = 10 ** (Math.max(-60, Math.min(20, db)) / 20);
  return (1 - Math.sqrt((gain - 0.001) / 9.999)) * 100;
}
export function gainLineTenths(percent: number) {
  const progress = 1 - Math.max(0, Math.min(100, percent)) / 100;
  const db = 20 * Math.log10(0.001 + progress ** 2 * 9.999);
  return Math.max(-600, Math.min(200, Math.round(db / 0.1)));
}
