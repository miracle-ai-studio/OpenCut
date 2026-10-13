import { useEffect, useState } from "react";

type Entry = { x: number; y: number; w: number; ch: number };
type Atlas = { fonts: Record<string, Entry> };
let atlasPromise: Promise<Atlas> | undefined;
const loadAtlas = () => {
  atlasPromise ??= fetch("/assets/classic-font-atlas/font-atlas.json")
    .then((response) => {
      if (!response.ok) throw new Error("Font preview atlas unavailable");
      return response.json() as Promise<Atlas>;
    })
    .catch((error) => {
      atlasPromise = undefined;
      throw error;
    });
  return atlasPromise;
};

// Fixed Classic cf5e79e919144200294fb9fed22a222592a0aeea atlas;
// family specimens only. This does not load or authorize an importable font.
export function FontAtlasPreview({ family }: { family: string }) {
  const [atlas, setAtlas] = useState<Atlas | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    void loadAtlas()
      .then((value) => {
        if (live) setAtlas(value);
      })
      .catch(() => {
        if (live) setFailed(true);
      });
    return () => {
      live = false;
    };
  }, []);
  const entry = atlas?.fonts[family];
  return (
    <figure className="font-atlas-preview" aria-label="Font family preview">
      {entry ? (
        <div
          role="img"
          aria-label={`${family} family specimen`}
          style={{
            width: entry.w,
            height: 40,
            backgroundColor: "currentColor",
            maskImage: `url(/assets/classic-font-atlas/font-chunk-${entry.ch}.avif)`,
            maskPosition: `-${entry.x}px -${entry.y}px`,
            maskRepeat: "no-repeat",
            WebkitMaskImage: `url(/assets/classic-font-atlas/font-chunk-${entry.ch}.avif)`,
            WebkitMaskPosition: `-${entry.x}px -${entry.y}px`,
            WebkitMaskRepeat: "no-repeat",
          }}
        />
      ) : (
        <p aria-live="polite">
          {failed
            ? "Family preview unavailable."
            : atlas
              ? "No Classic family specimen."
              : "Loading family preview…"}
        </p>
      )}
      <figcaption>
        Classic family specimen. Selected weight, style and your text are
        checked during import and rendering.
      </figcaption>
    </figure>
  );
}
