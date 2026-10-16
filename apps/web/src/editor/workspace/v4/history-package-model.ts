import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
export type Inspection = ToolOutput<"opencut_project_v4_history_package_inspect">;
export type ImportRequest = ToolInput<"opencut_project_v4_history_package_import">;
const identity=(value:unknown):value is string=>typeof value==="string"&&value.length>0&&value.length<=128&&!/[\u0000-\u001f\u007f]/.test(value);
export function validateHistoryPackage(value: Inspection) {
  if (!/^[a-f0-9]{64}$/.test(value.manifest_sha256) || value.history_included !== true ||
    value.self_contained_resources !== true || value.fully_reproducible !== false ||
    !identity(value.source_project_id) || !identity(value.source_revision) || typeof value.project_name!=="string" || value.project_name.length>2048 ||
    !Number.isSafeInteger(value.revision_count) || value.revision_count < 1 ||
    [value.transaction_count,value.branch_count,value.merge_count,value.asset_version_count,value.font_version_count,value.derived_font_count].some(n=>!Number.isSafeInteger(n)||n<0))
    throw Error("The history package inspection is incomplete.");
}
export function historyPackageRestoreRequest(path: string, value: Inspection): ImportRequest {
  validateHistoryPackage(value);
  return {package_path:path,target_project_id:crypto.randomUUID(),expected_manifest_sha256:value.manifest_sha256,idempotency_key:crypto.randomUUID()};
}
export function verifyHistoryPackageReceipt(request: ImportRequest, inspection: Inspection, receipt: ToolOutput<"opencut_project_v4_history_package_import">) {
  validateHistoryPackage(receipt.package);
  const fields = ["manifest_sha256","source_project_id","source_revision","project_name","revision_count","transaction_count","branch_count","merge_count","asset_version_count","font_version_count","derived_font_count"] as const;
  if(receipt.target_project_id!==request.target_project_id || !identity(receipt.revision) || request.expected_manifest_sha256!==inspection.manifest_sha256 || fields.some(field=>receipt.package[field]!==inspection[field]))
    throw Error("The restored history differs from the reviewed package.");
}
