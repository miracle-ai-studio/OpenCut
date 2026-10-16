import { useState } from "react";
import { ExportHistoryProjectPackage, RestoreHistoryProjectPackage } from "./history-package";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import { useWorkspace } from "../context";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";

type Inspection = ToolOutput<"opencut_project_v4_package_inspect">;
type ImportRequest = ToolInput<"opencut_project_v4_package_import">;
export type PackageSource = {
  project_id: string;
  revision: string;
  name: string;
};
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
export function packageRestoreRequest(
  packagePath: string,
  inspection: Inspection,
): ImportRequest {
  if (
    !/^[a-f0-9]{64}$/.test(inspection.manifest_sha256) ||
    !inspection.self_contained_resources ||
    inspection.history_included
  )
    throw new Error(
      "This package cannot be restored as a complete saved revision.",
    );
  return {
    package_path: packagePath,
    expected_manifest_sha256: inspection.manifest_sha256,
    target_project_id: crypto.randomUUID(),
    idempotency_key: crypto.randomUUID(),
  };
}
export function verifyPackageReceipt(
  request: ImportRequest,
  inspection: Inspection,
  receipt: ToolOutput<"opencut_project_v4_package_import">,
) {
  if (
    receipt.target_project_id !== request.target_project_id ||
    receipt.manifest_sha256 !== request.expected_manifest_sha256 ||
    receipt.source_project_id !== inspection.source_project_id ||
    receipt.source_revision !== inspection.source_revision ||
    receipt.asset_count !== inspection.asset_count ||
    receipt.font_count !== inspection.font_count ||
    receipt.derived_font_count !== inspection.derived_font_count ||
    !receipt.revision ||
    !receipt.self_contained_resources ||
    receipt.history_included ||
    receipt.fully_reproducible
  )
    throw new Error(
      "The restored project receipt does not match the reviewed package.",
    );
}
function Summary({
  value,
}: {
  value: Pick<Inspection, "asset_count" | "font_count" | "derived_font_count">;
}) {
  return (
    <p>
      {t("{media} media files · {fonts} fonts · {proofs} font source records", {
        media: value.asset_count,
        fonts: value.font_count,
        proofs: value.derived_font_count,
      })}
    </p>
  );
}
export function ExportProjectPackage({
  source,
  onClose,
}: {
  source: PackageSource;
  onClose: () => void;
}) {
  useLocale();
  const { api, controller } = useWorkspace();
  const [openedApi] = useState(api);
  const [withHistory,setWithHistory]=useState(false);
  const [destination, setDestination] = useState("");
  const [attempt, setAttempt] =
    useState<ToolInput<"opencut_project_v4_package_export"> | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [result, setResult] = useState<
    Inspection | ToolOutput<"opencut_project_v4_package_export"> | null
  >(null);
  const current = api && api === openedApi;
  const stillCurrent = () => controller.getSnapshot().api === openedApi;
  const exportPackage = async () => {
    if (!current || busy || attempt) return;
    const request = {
      project_id: source.project_id,
      revision: source.revision,
      destination: destination.trim(),
    };
    setAttempt(request);
    setBusy(true);
    setError("");
    try {
      const receipt = await api.sdk.call(
        "opencut_project_v4_package_export",
        request,
      );
      if (!stillCurrent()) return;
      if (
        receipt.source_revision !== source.revision ||
        !receipt.self_contained_resources ||
        receipt.history_included
      )
        throw Error(
          "The package receipt does not match the selected revision.",
        );
      setResult(receipt);
    } catch (e) {
      if (stillCurrent()) setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const checkDestination = async () => {
    if (!current || !attempt || busy) return;
    setBusy(true);
    setError("");
    try {
      const receipt = await api.sdk.call("opencut_project_v4_package_inspect", {
        package_path: attempt.destination,
      });
      if (!stillCurrent()) return;
      if (
        receipt.source_project_id !== attempt.project_id ||
        receipt.source_revision !== attempt.revision
      )
        throw Error("The destination contains a different project revision.");
      setResult(receipt);
    } catch (e) {
      if (stillCurrent()) setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  if(withHistory)return <ExportHistoryProjectPackage source={source} onClose={onClose} onBack={()=>setWithHistory(false)}/>;
  return (
    <Modal
      title={t("Export project package")}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <p>{source.name}</p>
      {!attempt&&<Button onClick={()=>setWithHistory(true)}>{t("Include edit history")}</Button>}
      <p>
        {t(
          "Save this revision with all media and fonts. Edit history is not included.",
        )}
      </p>
      <Field label={t("New package folder")}>
        <input
          value={destination}
          disabled={busy || !!attempt}
          placeholder="/Users/you/Projects/My project package"
          onChange={(e) => setDestination(e.target.value)}
        />
      </Field>
      <p className="muted">
        {t(
          "Enter an absolute folder path on the computer running OpenCut. The folder must not already exist.",
        )}
      </p>
      {!current && (
        <p role="alert">
          {t("The connection changed. Reopen this dialog before continuing.")}
        </p>
      )}
      {error && <p role="alert">{error}</p>}
      {result && (
        <div role="status">
          <strong>{t("Project package saved and verified")}</strong>
          <Summary value={result} />
          <p>{attempt?.destination}</p>
        </div>
      )}
      <div className="modal-footer">
        <Button disabled={busy} onClick={onClose}>
          {t(result ? "Done" : "Close")}
        </Button>
        {attempt && !result && (
          <Button
            disabled={!current || busy}
            onClick={() => void checkDestination()}
          >
            {t("Check destination")}
          </Button>
        )}
        {!attempt && (
          <Button
            className="primary"
            disabled={!current || busy || !destination.trim().startsWith("/")}
            onClick={() => void exportPackage()}
          >
            {t(busy ? "Saving package…" : "Save package")}
          </Button>
        )}
      </div>
    </Modal>
  );
}
export function RestoreProjectPackage({
  onClose,
  onOpen,
}: {
  onClose: () => void;
  onOpen: () => void;
}) {
  useLocale();
  const { api, controller } = useWorkspace();
  const [openedApi] = useState<EditorApi | null>(api);
  const [withHistory,setWithHistory]=useState(false);
  const [packagePath, setPackagePath] = useState("");
  const [review, setReview] = useState<{
    path: string;
    value: Inspection;
  } | null>(null);
  const [attempt, setAttempt] = useState<ImportRequest | null>(null);
  const [receipt, setReceipt] =
    useState<ToolOutput<"opencut_project_v4_package_import"> | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const current = api && api === openedApi;
  const stillCurrent = () => controller.getSnapshot().api === openedApi;
  const inspect = async () => {
    if (!current || busy || attempt) return;
    const path = packagePath.trim();
    setBusy(true);
    setError("");
    setReview(null);
    try {
      const value = await api.sdk.call("opencut_project_v4_package_inspect", {
        package_path: path,
      });
      if (!stillCurrent()) return;
      setReview({ path, value });
    } catch (e) {
      if (stillCurrent()) setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const restore = async () => {
    if (!current || busy || !review) return;
    setBusy(true);
    setError("");
    try {
      const request =
        attempt ?? packageRestoreRequest(review.path, review.value);
      setAttempt(request);
      const result = await api.sdk.call(
        "opencut_project_v4_package_import",
        request,
      );
      if (!stillCurrent()) return;
      verifyPackageReceipt(request, review.value, result);
      setReceipt(result);
      await controller.refreshProjects();
    } catch (e) {
      if (stillCurrent()) setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const open = async () => {
    if (!current || busy || !receipt || !review) return;
    setBusy(true);
    setError("");
    try {
      await controller.open({
        id: receipt.target_project_id,
        revision: receipt.revision,
        name: review.value.project_name,
        format: "v4",
      });
      if (!stillCurrent()) return;
      onClose();
      onOpen();
    } catch (e) {
      if (stillCurrent()) setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  if(withHistory)return <RestoreHistoryProjectPackage onClose={onClose} onOpen={onOpen} onBack={()=>setWithHistory(false)}/>;
  return (
    <Modal
      title={t("Restore project package")}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <p>
        {t(
          "Restore a saved revision as a new project, including its media and fonts.",
        )}
      </p>
      {!attempt&&<Button onClick={()=>setWithHistory(true)}>{t("Restore a package with edit history")}</Button>}
      <Field label={t("Package folder")}>
        <input
          value={packagePath}
          disabled={busy || !!attempt}
          onChange={(e) => {
            setPackagePath(e.target.value);
            setReview(null);
            setError("");
          }}
        />
      </Field>
      <p className="muted">
        {t("Enter an absolute folder path on the computer running OpenCut.")}
      </p>
      {!current && (
        <p role="alert">
          {t("The connection changed. Reopen this dialog before continuing.")}
        </p>
      )}
      {review && (
        <div>
          <h3>{review.value.project_name}</h3>
          <Summary value={review.value} />
          <p>
            {t(
              "This package contains one saved revision. Previous edit history is not included.",
            )}
          </p>
        </div>
      )}
      {error && <p role="alert">{error}</p>}
      {attempt && !receipt && !busy && (
        <p>
          {t(
            "Retry uses the same restore request and cannot create a second project.",
          )}
        </p>
      )}
      {receipt && <p role="status">{t("Project restored")}</p>}
      <div className="modal-footer">
        <Button disabled={busy} onClick={onClose}>
          {t("Close")}
        </Button>
        {!review && (
          <Button
            disabled={!current || busy || !packagePath.trim().startsWith("/")}
            onClick={() => void inspect()}
          >
            {t(busy ? "Checking package…" : "Check package")}
          </Button>
        )}
        {review && !receipt && (
          <Button
            className="primary"
            disabled={!current || busy}
            onClick={() => void restore()}
          >
            {t(
              busy
                ? "Restoring…"
                : attempt
                  ? "Retry restore"
                  : "Restore as new project",
            )}
          </Button>
        )}
        {receipt && (
          <Button
            className="primary"
            disabled={!current || busy}
            onClick={() => void open()}
          >
            {t("Open restored project")}
          </Button>
        )}
      </div>
    </Modal>
  );
}
