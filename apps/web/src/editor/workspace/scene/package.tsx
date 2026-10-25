import { OpenCutApiError } from "../../../lib/editor-sdk";
import { ErrorNotice } from "../../shared/error-notice";
import { Select } from "../../shared/controls";
import { useRef, useState } from "react";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import { useWorkspace } from "../context";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { validatePackage, packageRestoreRequest, verifyPackageReceipt, type Inspection, type ImportRequest } from "./package-model";
import { PackageRecovery } from "./package-recovery";
import { choosePackageLocation, packageFileName } from "./package-location";
export { packageRestoreRequest, verifyPackageReceipt } from "./package-model";
export type PackageSource = { project_id:string; revision:string; name:string };
const message=(value:unknown)=>value instanceof Error?value.message:String(value);
function Summary({value}:{value:Inspection}) {
  return <div><p>{t("{media} media files · {fonts} fonts · {proofs} font source records",{media:value.asset_version_count,fonts:value.font_version_count,proofs:value.derived_font_count})}</p>
    {value.scope==="history"&&<p>{t("{revisions} revisions · {edits} edits · {branches} branches",{revisions:value.revision_count,edits:value.transaction_count,branches:value.branch_count})}</p>}
    {value.mode==="linked"&&<p>{t("Linked media must remain available on this computer until restoration completes.")}</p>}
    {value.external_font_families.length>0&&<p>{t("External font families")}: {value.external_font_families.join(", ")}</p>}
    {value.fonts_missing.length>0&&<p role="alert">{t("Missing fonts")}: {value.fonts_missing.join(", ")}</p>}
    {!value.tool_environment_matches&&<p>{t("This project was saved with different rendering tools. Its appearance may differ slightly on this computer.")}</p>}
    {value.plugin_bundle_count>0&&<p>{t("{count} plugin packages included. Installation and permissions require separate review.",{count:value.plugin_bundle_count})}</p>}
    {!value.authoring_available&&<p>{t("You can view this project. Convert it from the project library to make changes.")}</p>}
  </div>;
}
export function ExportProjectPackage({source,onClose}:{source:PackageSource;onClose():void}) {
  const {language}=useLocale();const {api,controller}=useWorkspace();
  const [captured]=useState(()=>({api,connection:api?.connectionIdentity,source:structuredClone(source)}));
  const [destination,setDestination]=useState(""),[scope,setScope]=useState<"saved_revision"|"history">("saved_revision"),[mode,setMode]=useState<"self_contained"|"linked">("self_contained");
  const [attempt,setAttempt]=useState<ToolInput<"opencut_project_package_export">|null>(null),[result,setResult]=useState<Inspection|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false);
  const [previousDestination, setPreviousDestination] = useState<string | null>(null);
  const [choosing,setChoosing]=useState(false);
  const flight=useRef(false),current=()=>{const active=controller.getSnapshot().api;return !!active&&active===captured.api&&active.connectionIdentity===captured.connection;};
  const choose=async(replace=false)=>{
    if(flight.current||!current())return;flight.current=true;setChoosing(true);
    try{
      const path=await choosePackageLocation(captured.api!,{purpose:"save",suggested_name:packageFileName(captured.source.name),language});
      if(!current()||path===null)return;
      if(replace&&attempt){setPreviousDestination(attempt.destination);setAttempt(null);}
      setDestination(path);setError(null);
    }catch(e){if(current())setError(message(e));}finally{flight.current=false;setChoosing(false);}
  };
  const run=async(checkOnly=false)=>{
    if(flight.current||!current())return;flight.current=true;setBusy(true);setError(null);
    const request=attempt??{project_id:captured.source.project_id,revision:captured.source.revision,destination:destination,scope,mode};
    if(!request.destination.startsWith("/")){flight.current=false;setBusy(false);setError("Enter an absolute folder path on the computer running OpenCut.");return;}
    setAttempt(request);
    try{
      const value=checkOnly?await captured.api!.sdk.call("opencut_project_package_inspect",{package_path:request.destination})
        :(await captured.api!.sdk.call("opencut_project_package_export",request)).package;
      if(!current())return;validatePackage(value);
      if(value.source_project_id!==request.project_id||value.source_revision!==request.revision||value.scope!==request.scope||value.mode!==request.mode)throw Error("The destination contains a different project revision.");
      setResult(value);
    }catch(e){if(current()){setError(message(e)); if (!checkOnly && e instanceof OpenCutApiError && ["VALIDATION_ERROR", "INVALID_ARGUMENT", "INVALID_INPUT", "PERMISSION_DENIED", "NOT_FOUND", "ALREADY_EXISTS"].includes(e.code)) setAttempt(null);}}finally{flight.current=false;setBusy(false);}
  };
  return <Modal title={t("Export project package")} onClose={()=>{if(!busy&&!choosing)onClose();}}>
    <p>{captured.source.name}</p>
    <Field label={t("Package content")}><Select disabled={busy||!!attempt} value={scope} onChange={e=>setScope(e.target.value as typeof scope)}><option value="saved_revision">{t("Saved revision only")}</option><option value="history">{t("Include edit history")}</option></Select></Field>
    <Field label={t("Media storage")}><Select disabled={busy||!!attempt} value={mode} onChange={e=>setMode(e.target.value as typeof mode)}><option value="self_contained">{t("Include media and fonts")}</option><option value="linked">{t("Link media on this computer")}</option></Select></Field>
    {mode==="linked"&&<p>{t("Linked media must remain available on this computer until restoration completes.")}</p>}
    <Field label={t("Save location on this computer")}><span role="group" className="package-location" aria-label={t("Save location on this computer")}>{destination||t("No location selected")}</span></Field>
    {!attempt&&<Button disabled={busy||choosing||!current()} onClick={()=>void choose()}>{t(choosing?"Choosing location…":destination?"Change save location…":"Choose save location…")}</Button>}
    <p className="muted">{t("Choose a name and location in the save window. OpenCut saves the project, media and fonts as a new project package folder. Existing content is never overwritten.")}</p>
    {choosing&&<p role="status">{t("Choose the location in the system window, or cancel to return here.")}</p>}
    {!current()&&<p role="alert">{t("The connection changed. Reopen this dialog before continuing.")}</p>}
    {error&&<ErrorNotice error={error}/>}
    {previousDestination && <p className="diagnostic-details">{t("Previous save location")}: {previousDestination}</p>}
    {attempt && !result && error && <div><p>{t("The original location may already contain a copy. Saving elsewhere creates another copy and leaves the original folder unchanged.")}</p><p className="diagnostic-details">{attempt.destination}</p><Button disabled={busy || choosing || !current()} onClick={() => void choose(true)}>{t("Save to another location")}</Button></div>}
    {result&&<div role="status"><p>{t("Project package saved and verified")}</p><p>{attempt?.destination}</p><Summary value={result}/></div>}
    <div className="modal-footer"><Button disabled={busy||choosing} onClick={onClose}>{t(result?"Done":"Close")}</Button>
      {attempt&&!result&&<><p>{t("The save result is not confirmed. Check the folder or retry saving to the same location.")}</p><Button disabled={busy||choosing||!current()} onClick={()=>void run(true)}>{t("Check destination")}</Button><Button disabled={busy||choosing||!current()} onClick={()=>void run()}>{t("Retry save")}</Button></>}
      {!attempt&&<Button className="primary" disabled={busy||choosing||!current()||!destination.startsWith("/")} onClick={()=>void run()}>{t("Save package")}</Button>}
    </div>
  </Modal>;
}
export function RestoreProjectPackage({onClose,onOpen}:{onClose():void;onOpen():void}) {
  const {language}=useLocale();const {api,controller}=useWorkspace();
  const [captured]=useState(()=>({api,connection:api?.connectionIdentity}));
  const [recovery]=useState(()=>new PackageRecovery());
  const [restored]=useState(()=>{try{return {record:captured.connection?recovery.read(captured.connection):null,error:null};}catch(e){return {record:null,error:message(e)};}});
  const [path,setPath]=useState(restored.record?.request.package_path??""),[review,setReview]=useState<{path:string;value:Inspection}|null>(restored.record?{path:restored.record.request.package_path,value:restored.record.inspection}:null),
    [attempt,setAttempt]=useState<ImportRequest|null>(restored.record?.request??null),[receipt,setReceipt]=useState<ToolOutput<"opencut_project_package_import">|null>(null),
    [busy,setBusy]=useState(false),[error,setError]=useState<string|null>(restored.error);
  const flight=useRef(false),current=()=>{const active=controller.getSnapshot().api;return !!active&&active===captured.api&&active.connectionIdentity===captured.connection;};
  const run=async(action:()=>Promise<void>)=>{if(flight.current||!current())return;flight.current=true;setBusy(true);setError(null);try{await action();}catch(e){if(current())setError(message(e));}finally{flight.current=false;setBusy(false);}};
  const inspect=()=>run(async()=>{const requested=path,value=await captured.api!.sdk.call("opencut_project_package_inspect",{package_path:requested});if(!current())return;validatePackage(value);setReview({path:requested,value});});
  const choose=()=>run(async()=>{const selected=await choosePackageLocation(captured.api!,{purpose:"open",suggested_name:null,language});if(!current()||selected===null)return;setPath(selected);setReview(null);setError(null);});
  const restore=()=>run(async()=>{
    if(!review)return;const request=attempt??packageRestoreRequest(review.path,review.value);
    if(!captured.connection||restored.error)throw Error(restored.error??"Authenticate the service before restoring.");
    recovery.write(captured.connection,{request,inspection:review.value});setAttempt(request);
    const value=await captured.api!.sdk.call("opencut_project_package_import",request);if(!current())return;
    verifyPackageReceipt(request,review.value,value);setReceipt(value);recovery.remove(captured.connection,request.idempotency_key);await controller.refreshProjects();
  });
  const open=()=>run(async()=>{if(!receipt||!review)return;await controller.open({id:receipt.target_project_id,revision:receipt.revision,name:review.value.project_name,...(receipt.package.authoring_available?{format:"editor" as const}:{})});if(current()){onClose();onOpen();}});
  return <Modal title={t("Restore project package")} onClose={()=>{if(!busy)onClose();}}>
    <p>{t("Restore saved content and any included edit history as a new project.")}</p>
    <Field label={t("Package folder")}><span role="group" className="package-location" aria-label={t("Package folder")}>{path||t("No package selected")}</span></Field>
    <Button disabled={busy||!!attempt||!current()} onClick={()=>void choose()}>{t(path?"Choose another package…":"Choose project package…")}</Button>
    <p className="muted">{t("Select the project package folder in the system window, then check it before restoring.")}</p>
    {!current()&&<p role="alert">{t("The connection changed. Reopen this dialog before continuing.")}</p>}
    {review&&<><h3>{review.value.project_name}</h3><Summary value={review.value}/></>}{error&&<ErrorNotice error={error}/>}
    {attempt&&!receipt&&!busy&&<p>{t("Retry uses the same restore request and cannot create a second project.")}</p>}
    {receipt&&<><p role="status">{t("Project restored")}</p>{receipt.plugin_restore_plan.length>0&&<details><summary>{t("Review plugin installation and permissions")}</summary><p>{t("No plugins or permissions were applied. Review these actions separately.")}</p><pre>{JSON.stringify(receipt.plugin_restore_plan,null,2)}</pre></details>}</>}
    <div className="modal-footer"><Button disabled={busy} onClick={onClose}>{t("Close")}</Button>
      {!review&&<Button disabled={busy||!current()||!path.trim().startsWith("/")} onClick={()=>void inspect()}>{t("Check package")}</Button>}
      {review&&!receipt&&<Button className="primary" disabled={busy||!current()} onClick={()=>void restore()}>{t(attempt?"Retry restore":"Restore as new project")}</Button>}
      {receipt&&<Button className="primary" disabled={busy||!current()} onClick={()=>void open()}>{t("Open restored project")}</Button>}
    </div>
  </Modal>;
}
