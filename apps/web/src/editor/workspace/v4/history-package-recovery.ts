import type { Inspection, ImportRequest } from "./history-package-model";
import { validateHistoryPackage } from "./history-package-model";
import { canonicalRecoveryIdentity, validRecoveryId } from "./edit-recovery-pointers";
type Saved = {request:ImportRequest;inspection:Inspection};
type Envelope = {version:1;entries:{identity:string;value:Saved}[]};
type StoragePort=Pick<Storage,"getItem"|"setItem">;
const invalid=()=>Error("History package recovery is unavailable or invalid. Resolve the original restore before starting another.");
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==="object"&&!Array.isArray(value);
const exact=(value:unknown,keys:string):value is Record<string,unknown>=>object(value)&&Object.keys(value).sort().join()===keys;
function saved(value:unknown):Saved {
  if(!exact(value,"inspection,request")||!exact(value.request,"expected_manifest_sha256,idempotency_key,package_path,target_project_id"))throw invalid();
  const r=value.request;
  if(typeof r.package_path!=="string"||!r.package_path.startsWith("/")||new TextEncoder().encode(r.package_path).length>4096||/[\u0000-\u001f\u007f]/.test(r.package_path)||!validRecoveryId(r.target_project_id)||!validRecoveryId(r.idempotency_key)||typeof r.expected_manifest_sha256!=="string"||!/^[a-f0-9]{64}$/.test(r.expected_manifest_sha256))throw invalid();
  if(!exact(value.inspection,"asset_version_count,branch_count,derived_font_count,font_version_count,fully_reproducible,history_included,manifest_sha256,merge_count,project_name,revision_count,self_contained_resources,source_project_id,source_revision,transaction_count"))throw invalid();
  const inspection=value.inspection as unknown as Inspection;
  if(!validRecoveryId(inspection.source_project_id)||!validRecoveryId(inspection.source_revision)||typeof inspection.project_name!=="string"||inspection.project_name.length>2048||inspection.manifest_sha256!==r.expected_manifest_sha256)throw invalid();
  validateHistoryPackage(inspection);
  return {request:{package_path:r.package_path,target_project_id:r.target_project_id,idempotency_key:r.idempotency_key,expected_manifest_sha256:r.expected_manifest_sha256},inspection:structuredClone(inspection)};
}
function parse(raw:string|null):Envelope {
  if(raw===null)return {version:1,entries:[]};
  if(new TextEncoder().encode(raw).length>65536)throw invalid();
  const value:unknown=JSON.parse(raw);
  if(!exact(value,"entries,version")||value.version!==1||!Array.isArray(value.entries)||value.entries.length>4)throw invalid();
  const seen=new Set<string>();
  return {version:1,entries:value.entries.map(entry=>{if(!exact(entry,"identity,value")||typeof entry.identity!=="string")throw invalid();const identity=canonicalRecoveryIdentity(entry.identity);if(seen.has(identity))throw invalid();seen.add(identity);return {identity,value:saved(entry.value)};})};
}
/** One bounded original owner-local restore per public connection identity.
 * Retains only the user-entered package folder, original public request and
 * reviewed counts/digest. No project document, blob or authentication secret. */
export class HistoryPackageRecoveryV1 {
  constructor(private readonly storage?:StoragePort){}
  private readRaw(){return this.storage?this.storage.getItem("opencut.history-package.v1"):sessionStorage.getItem("opencut.history-package.v1");}
  private writeRaw(raw:string){if(this.storage)this.storage.setItem("opencut.history-package.v1",raw);else sessionStorage.setItem("opencut.history-package.v1",raw);if(this.readRaw()!==raw)throw invalid();}
  private entries(){try{return parse(this.readRaw());}catch{throw invalid();}}
  private persist(value:Envelope){const raw=JSON.stringify(value);parse(raw);this.writeRaw(raw);}
  read(scope:string){const identity=canonicalRecoveryIdentity(scope);return this.entries().entries.find(entry=>entry.identity===identity)?.value??null;}
  write(scope:string,value:Saved){const identity=canonicalRecoveryIdentity(scope),data=this.entries(),original=data.entries.find(entry=>entry.identity===identity),next=saved(value);if(original&&JSON.stringify(original.value)!==JSON.stringify(next))throw invalid();if(!original)data.entries.push({identity,value:next});this.persist(data);}
  remove(scope:string,key:string){const identity=canonicalRecoveryIdentity(scope),data=this.entries();data.entries=data.entries.filter(entry=>entry.identity!==identity||entry.value.request.idempotency_key!==key);this.persist(data);}
}
