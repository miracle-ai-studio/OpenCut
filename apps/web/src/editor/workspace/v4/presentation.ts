import type { Time } from "../../model/types";
import type { V4Bookmark } from "./types";
import { compareTime } from "../../model/time";

/** HUD visibility is a read-only presentation rule matching Classic's inclusive
 * endpoints. Keep intermediate rational sums wider than the public Time DTO. */
export function activeBookmarkNotes(bookmarks: V4Bookmark[], now: Time) {
  return bookmarks.filter((b) => {
    if (!b.note?.trim() || compareTime(now, b.time) < 0) return false;
    const d = b.duration ?? { ticks: "0", timebase: 1 };
    const end =
      BigInt(b.time.ticks) * BigInt(d.timebase) +
      BigInt(d.ticks) * BigInt(b.time.timebase);
    return (
      BigInt(now.ticks) * BigInt(b.time.timebase) * BigInt(d.timebase) <=
      end * BigInt(now.timebase)
    );
  });
}

export function formatTimelineEnd(seconds: number) {
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${(seconds % 60).toFixed(2).padStart(5, "0")}`;
}
