import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
export type Inspection = ToolOutput<"opencut_project_package_inspect">;
export type ImportRequest = ToolInput<"opencut_project_package_import">;
export const packageInspectionKeys = "asset_version_count,authoring_available,branch_count,derived_font_count,external_font_families,font_version_count,fonts_missing,manifest_sha256,merge_count,mode,plugin_bundle_count,project_name,revision_count,scope,source_project_id,source_revision,tool_environment_matches,tool_lock_mismatches,tool_locks,transaction_count";
const identity=(value:unknown):value is string=>typeof value==="string"&&value.length>0&&value.length<=1024&&!/[\u0000-\u001f\u007f]/.test(value);
export function validatePackage(value: Inspection) {
  if (Object.keys(value).sort().join()!==packageInspectionKeys || !/^[a-f0-9]{64}$/.test(value.manifest_sha256) || !identity(value.source_project_id) || !identity(value.source_revision) ||
    typeof value.project_name!=="string" || value.project_name.length>2048 || typeof value.authoring_available!=="boolean" ||
    !["linked","self_contained"].includes(value.mode) || !["saved_revision","history"].includes(value.scope) ||
    !Number.isSafeInteger(value.revision_count) || value.revision_count<1 ||
    [value.transaction_count,value.branch_count,value.merge_count,value.asset_version_count,value.font_version_count,value.derived_font_count,value.plugin_bundle_count].some(n=>!Number.isSafeInteger(n)||n<0) ||
    (value.scope==="saved_revision" && (value.revision_count!==1 || value.transaction_count!==0 || value.branch_count!==0 || value.merge_count!==0)) ||
    typeof value.tool_environment_matches!=="boolean" ||
    [value.external_font_families,value.fonts_missing,value.tool_lock_mismatches].some(v=>!Array.isArray(v)||v.some(s=>!identity(s))) ||
    !Array.isArray(value.tool_locks) || value.tool_locks.some(t=>!identity(t.name)||!identity(t.version)||!/^[a-f0-9]{64}$/.test(t.binary_sha256)))
    throw Error("The package inspection is incomplete.");
}
export function packageRestoreRequest(path:string,inspection:Inspection):ImportRequest {
  validatePackage(inspection);
  return {package_path:path,target_project_id:crypto.randomUUID(),expected_manifest_sha256:inspection.manifest_sha256,idempotency_key:crypto.randomUUID()};
}
export function verifyPackageReceipt(request:ImportRequest,inspection:Inspection,receipt:ToolOutput<"opencut_project_package_import">) {
  try { validatePackage(receipt.package); } catch { throw Error("The restored project receipt does not match the reviewed package."); }
  const fields=["manifest_sha256","source_project_id","source_revision","project_name","mode","scope","authoring_available","revision_count","transaction_count","branch_count","merge_count","asset_version_count","font_version_count","derived_font_count","plugin_bundle_count"] as const;
  if(receipt.target_project_id!==request.target_project_id || !identity(receipt.revision) || request.expected_manifest_sha256!==inspection.manifest_sha256 ||
    fields.some(field=>receipt.package[field]!==inspection[field]) || !Array.isArray(receipt.plugin_restore_plan))
    throw Error("The restored project receipt does not match the reviewed package.");
}
