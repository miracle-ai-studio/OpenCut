export function MediaArt({
  image,
  peaks,
  from = 0,
  to = 1,
}: {
  image?: string;
  peaks?: number[];
  from?: number;
  to?: number;
}) {
  const visible = peaks?.slice(
    Math.floor(Math.max(0, from) * peaks.length),
    Math.max(1, Math.ceil(Math.min(1, to) * peaks.length)),
  );
  return image ? (
    <img className="media-art" src={image} alt="" draggable={false} />
  ) : visible?.length ? (
    <svg
      className="media-waveform"
      viewBox="0 0 256 48"
      preserveAspectRatio="none"
      aria-hidden
    >
      {visible.map((peak, i) => (
        <line
          key={i}
          x1={(i / visible.length) * 256}
          x2={(i / visible.length) * 256}
          y1={24 - Math.max(1, peak * 23)}
          y2={24 + Math.max(1, peak * 23)}
        />
      ))}
    </svg>
  ) : null;
}
