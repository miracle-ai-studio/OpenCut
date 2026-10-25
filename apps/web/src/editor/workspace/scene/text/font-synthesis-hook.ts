import { useEffect, useMemo, useState } from "react";
import type { EditorApi } from "../../api";
import type { TextFont, TextScope } from "./types";
import { enrichSynthesisFonts, synthesisFont } from "./font-synthesis";

/** Volatile proof enrichment is scoped to this exact API and immutable revision. */
export function useSynthesisFonts(
  api: EditorApi | null | undefined,
  scope: Pick<TextScope, "projectId" | "revision"> | null,
  fonts: readonly TextFont[],
) {
  const [resolved, setResolved] = useState<{
    api: EditorApi;
    identity: string | null;
    project: string;
    revision: string;
    original: string;
    fonts: TextFont[];
    error: string | null;
  } | null>(null);
  // Polling may deserialize the same immutable revision into fresh objects.
  // Only complete resource-content changes invalidate its scoped proof; object
  // identity must not repeatedly disable and cancel an in-flight Canvas plan.
  const original = useMemo(() => JSON.stringify(fonts), [fonts]);
  const needed = fonts.some(synthesisFont);
  const current =
    resolved &&
    resolved.api === api &&
    resolved.identity === api?.connectionIdentity &&
    resolved.project === scope?.projectId &&
    resolved.revision === scope?.revision &&
    resolved.original === original
      ? resolved
      : null;
  useEffect(() => {
    if (!needed || !api || !scope) return;
    const abort = new AbortController(),
      identity = api.connectionIdentity;
    void enrichSynthesisFonts(api, scope, fonts, abort.signal)
      .then((value) => {
        if (!abort.signal.aborted && api.connectionIdentity === identity)
          setResolved({
            api,
            identity,
            project: scope.projectId,
            revision: scope.revision,
            original,
            fonts: value,
            error: null,
          });
      })
      .catch((error) => {
        if (!abort.signal.aborted && api.connectionIdentity === identity)
          setResolved({
            api,
            identity,
            project: scope.projectId,
            revision: scope.revision,
            original,
            fonts: fonts.filter((font) => !synthesisFont(font)),
            error: error instanceof Error ? error.message : String(error),
          });
      });
    return () => abort.abort();
  }, [
    api,
    api?.connectionIdentity,
    scope?.projectId,
    scope?.revision,
    original,
    needed,
  ]);
  return {
    fonts: !needed
      ? fonts
      : (current?.fonts ?? fonts.filter((font) => !synthesisFont(font))),
    loading: needed && !current,
    error: current?.error ?? null,
  };
}
