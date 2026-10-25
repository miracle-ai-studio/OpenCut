import type {
  UnifiedFontCatalog,
  UnifiedFontSourceReader,
} from "./font-catalog";
import type { TextScope } from "./types";
import { FontBrowserImport } from "./font-browser";
export type FontImportProps = {
  scope: Pick<TextScope, "projectId" | "revision">;
  catalog: UnifiedFontCatalog;
  readSource: UnifiedFontSourceReader;
  requiredText: string;
  disabled?: boolean;
  disabledReason?: string | null;
  importFont(choiceId: string, requiredText: string): Promise<unknown>;
  close(): void;
};
export function FontImport(props: FontImportProps) {
  return (
    <FontBrowserImport
      key={JSON.stringify([props.scope.projectId, props.scope.revision])}
      {...props}
    />
  );
}
