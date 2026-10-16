import { useRef, useState } from "react";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import { useWorkspace } from "../context";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import type { PackageSource } from "./package";
import { validateHistoryPackage, historyPackageRestoreRequest, verifyHistoryPackageReceipt, type Inspection, type ImportRequest } from "./history-package-model";
import { HistoryPackageRecoveryV1 } from "./history-package-recovery";
const message=(value:unknown)=>value instanceof Error?value.message:String(value);
function Summary({value}:{value:Inspection}) {
  return <div role="status"><p>{t("{revisions} revisions · {edits} edits · {branches} branches",{revisions:value.revision_count,edits:value.transaction_count,branches:value.branch_count})}</p>
    <p>{t("{media} media versions · {fonts} font versions",{media:value.asset_version_count,fonts:value.font_version_count})}</p>
    <p className="muted">{t("Restores saved content and edit history. Sessions, credentials and running tasks are excluded.")}</p></div>;
}
export function ExportHistoryProjectPackage({source,onClose,onBack}:{source:PackageSource;onClose():void;onBack():void}) {
  useLocale();const {api,controller}=useWorkspace();
  const [captured]=useState(()=>({api,connection:api?.connectionIdentity,source:structuredClone(source)}));
  const [destination,setDestination]=useState(""),[attempt,setAttempt]=useState<ToolInput<"opencut_project_v4_history_package_export">|null>(null),
    [result,setResult]=useState<Inspection|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false);
  const flight=useRef(false), current=()=>{const active=controller.getSnapshot().api;return !!active&&active===captured.api&&active.connectionIdentity===captured.connection;};
  const run=async(checkOnly=false)=>{
    if(flight.current||!current())return;flight.current=true;setBusy(true);setError(null);
    const request=attempt??{project_id:captured.source.project_id,expected_revision:captured.source.revision,destination:destination.trim()};
    if(!request.destination.startsWith("/")){flight.current=false;setBusy(false);setError("Enter an absolute folder path on the computer running OpenCut.");return;}
    setAttempt(request);
    try{
      const value=checkOnly?await captured.api!.sdk.call("opencut_project_v4_history_package_inspect",{package_path:request.destination})
        :(await captured.api!.sdk.call("opencut_project_v4_history_package_export",request)).package;
      if(!current())return;validateHistoryPackage(value);
      if(value.source_project_id!==request.project_id||value.source_revision!==request.expected_revision)throw Error("The destination contains a different project revision.");
      setResult(value);
    }catch(e){if(current())setError(message(e));}finally{flight.current=false;setBusy(false);}
  };
  return <Modal title={t("Export project with history")} onClose={()=>{if(!busy)onClose();}}>
    <p>{captured.source.name}</p><p>{t("Save the current project, every retained edit and all historical media and fonts.")}</p>
    <Field label={t("New package folder")}><input value={destination} disabled={busy||!!attempt} onChange={event=>setDestination(event.target.value)}/></Field>
    <p className="muted">{t("Enter an absolute folder path on the computer running OpenCut. The folder must not already exist.")}</p>
    {!current()&&<p role="alert">{t("The connection changed. Reopen this dialog before continuing.")}</p>}
    {error&&<p role="alert">{t(error)}</p>}{result&&<Summary value={result}/>}
    <div className="modal-footer"><Button disabled={busy} onClick={onClose}>{t("Close")}</Button>
      {!attempt&&<Button disabled={busy} onClick={onBack}>{t("Saved revision only")}</Button>}
      {attempt&&!result&&<Button disabled={busy||!current()} onClick={()=>void run(true)}>{t("Check destination")}</Button>}
      {!attempt&&<Button className="primary" disabled={busy||!current()||!destination.trim().startsWith("/")} onClick={()=>void run()}>{t("Save package")}</Button>}</div>
  </Modal>;
}
export function RestoreHistoryProjectPackage({onClose,onOpen,onBack}:{onClose():void;onOpen():void;onBack():void}) {
  useLocale();const {api,controller}=useWorkspace();
  const [captured]=useState(()=>({api,connection:api?.connectionIdentity}));
  const [recovery]=useState(()=>new HistoryPackageRecoveryV1());
  const [restored]=useState(()=>{try{return {record:captured.connection?recovery.read(captured.connection):null,error:null};}catch(e){return {record:null,error:message(e)};}});
  const [path,setPath]=useState(restored.record?.request.package_path??""),[review,setReview]=useState<{path:string;value:Inspection}|null>(restored.record?{path:restored.record.request.package_path,value:restored.record.inspection}:null),
    [attempt,setAttempt]=useState<ImportRequest|null>(restored.record?.request??null),[receipt,setReceipt]=useState<ToolOutput<"opencut_project_v4_history_package_import">|null>(null),
    [busy,setBusy]=useState(false),[error,setError]=useState<string|null>(restored.error);
  const flight=useRef(false),current=()=>{const active=controller.getSnapshot().api;return !!active&&active===captured.api&&active.connectionIdentity===captured.connection;};
  const run=async(action:()=>Promise<void>)=>{if(flight.current||!current())return;flight.current=true;setBusy(true);setError(null);try{await action();}catch(e){if(current())setError(message(e));}finally{flight.current=false;setBusy(false);}};
  const inspect=()=>run(async()=>{const requested=path.trim(),value=await captured.api!.sdk.call("opencut_project_v4_history_package_inspect",{package_path:requested});if(!current())return;validateHistoryPackage(value);setReview({path:requested,value});});
  const restore=()=>run(async()=>{if(!review)return;const request=attempt??historyPackageRestoreRequest(review.path,review.value);
    if(!captured.connection||restored.error)throw Error(restored.error??"Authenticate the service before restoring.");
    recovery.write(captured.connection,{request,inspection:review.value});setAttempt(request);
    const value=await captured.api!.sdk.call("opencut_project_v4_history_package_import",request);if(!current())return;verifyHistoryPackageReceipt(request,review.value,value);setReceipt(value);recovery.remove(captured.connection,request.idempotency_key);await controller.refreshProjects();});
  const open=()=>run(async()=>{if(!receipt||!review)return;await controller.open({id:receipt.target_project_id,revision:receipt.revision,name:review.value.project_name,format:"v4"});if(current()){onClose();onOpen();}});
  return <Modal title={t("Restore project with history")} onClose={()=>{if(!busy)onClose();}}>
    <p>{t("Restore a project and its retained edit history as a new project.")}</p>
    <Field label={t("Package folder")}><input value={path} disabled={busy||!!attempt} onChange={event=>{setPath(event.target.value);setReview(null);setError(null);}}/></Field>
    {!current()&&<p role="alert">{t("The connection changed. Reopen this dialog before continuing.")}</p>}
    {review&&<><h3>{review.value.project_name}</h3><Summary value={review.value}/></>}{error&&<p role="alert">{t(error)}</p>}
    {attempt&&!receipt&&!busy&&<p>{t("Retry uses the same restore request and cannot create a second project.")}</p>}
    {receipt&&<p role="status">{t("Project restored")}</p>}
    <div className="modal-footer"><Button disabled={busy} onClick={onClose}>{t("Close")}</Button>
      {!attempt&&<Button disabled={busy} onClick={onBack}>{t("Saved revision only")}</Button>}
      {!review&&<Button disabled={busy||!current()||!path.trim().startsWith("/")} onClick={()=>void inspect()}>{t("Check package")}</Button>}
      {review&&!receipt&&<Button className="primary" disabled={busy||!current()} onClick={()=>void restore()}>{t(attempt?"Retry restore":"Restore as new project")}</Button>}
      {receipt&&<Button className="primary" disabled={busy||!current()} onClick={()=>void open()}>{t("Open restored project")}</Button>}</div>
  </Modal>;
}
