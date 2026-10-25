import type { Inspection, ImportRequest } from "./package-model";
import { packageInspectionKeys, validatePackage } from "./package-model";
import { canonicalRecoveryIdentity, validRecoveryId } from "./edit-recovery-pointers";
type Saved={request:ImportRequest;inspection:Inspection};
type Envelope={entries:{identity:string;value:Saved}[]};
type StoragePort=Pick<Storage,"getItem"|"setItem">;
const invalid=()=>Error("Package recovery is unavailable or invalid. Resolve the original restore before starting another.");
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==="object"&&!Array.isArray(v);
const exact=(v:unknown,keys:string):v is Record<string,unknown>=>object(v)&&Object.keys(v).sort().join()===keys;
function saved(value:unknown):Saved {
  if(!exact(value,"inspection,request")||!exact(value.request,"expected_manifest_sha256,idempotency_key,package_path,target_project_id")||!exact(value.inspection,packageInspectionKeys))throw invalid();
  const r=value.request;
  if(typeof r.package_path!=="string"||!r.package_path.startsWith("/")||new TextEncoder().encode(r.package_path).length>4096||/[\u0000-\u001f\u007f]/.test(r.package_path)||!validRecoveryId(r.target_project_id)||!validRecoveryId(r.idempotency_key)||typeof r.expected_manifest_sha256!=="string"||!/^[a-f0-9]{64}$/.test(r.expected_manifest_sha256))throw invalid();
  const inspection=value.inspection as unknown as Inspection;
  validatePackage(inspection);
  if(inspection.manifest_sha256!==r.expected_manifest_sha256)throw invalid();
  return {request:{package_path:r.package_path,target_project_id:r.target_project_id,idempotency_key:r.idempotency_key,expected_manifest_sha256:r.expected_manifest_sha256},inspection:structuredClone(inspection)};
}
function parse(raw:string|null,legacy=false):Envelope {
  if(raw===null)return {entries:[]};
  if(new TextEncoder().encode(raw).length>65536)throw invalid();
  const value:unknown=JSON.parse(raw);
  if(!exact(value,legacy?"entries,version":"entries")||(legacy&&value.version!==1)||!Array.isArray(value.entries)||value.entries.length>4)throw invalid();
  const seen=new Set<string>();
  return {entries:value.entries.map(entry=>{
    if(!exact(entry,"identity,value")||typeof entry.identity!=="string")throw invalid();
    const identity=canonicalRecoveryIdentity(entry.identity);if(seen.has(identity))throw invalid();seen.add(identity);
    let data=entry.value;
    if(legacy){
      if(!exact(data,"inspection,request")||!exact(data.inspection,"asset_version_count,branch_count,derived_font_count,font_version_count,fully_reproducible,history_included,manifest_sha256,merge_count,project_name,revision_count,self_contained_resources,source_project_id,source_revision,transaction_count"))throw invalid();
      const old=data.inspection;
      if(old.history_included!==true||old.self_contained_resources!==true||old.fully_reproducible!==false)throw invalid();
      const {history_included:_,self_contained_resources:__,fully_reproducible:___,...facts}=old;
      data={request:data.request,inspection:{...facts,scope:"history",mode:"self_contained",authoring_available:true,plugin_bundle_count:0,external_font_families:[],fonts_missing:[],tool_locks:[],tool_environment_matches:false,tool_lock_mismatches:["ffmpeg","ffprobe"]}};
    }
    return {identity,value:saved(data)};
  })};
}
/** One bounded original restore per authenticated connection. Historical saved
 * requests are only read; a successful current write prevents resurrection. */
export class PackageRecovery {
  constructor(private readonly storage?:StoragePort){}
  private readRaw(){return this.storage?this.storage.getItem("opencut.package"):sessionStorage.getItem("opencut.package");}
  private readLegacy(){return this.storage?this.storage.getItem("opencut.history-package.v1"):sessionStorage.getItem("opencut.history-package.v1");}
  private entries(){try{const current=this.readRaw();return current===null?parse(this.readLegacy(),true):parse(current);}catch{throw invalid();}}
  private persist(value:Envelope){const raw=JSON.stringify(value);parse(raw);if(this.storage)this.storage.setItem("opencut.package",raw);else sessionStorage.setItem("opencut.package",raw);if(this.readRaw()!==raw)throw invalid();}
  read(scope:string){const identity=canonicalRecoveryIdentity(scope);return this.entries().entries.find(e=>e.identity===identity)?.value??null;}
  write(scope:string,value:Saved){const identity=canonicalRecoveryIdentity(scope),data=this.entries(),next=saved(value),original=data.entries.find(e=>e.identity===identity);if(original&&JSON.stringify(original.value)!==JSON.stringify(next))throw invalid();if(!original)data.entries.push({identity,value:next});this.persist(data);}
  remove(scope:string,originalKey:string){const identity=canonicalRecoveryIdentity(scope),data=this.entries();data.entries=data.entries.filter(e=>e.identity!==identity||e.value.request.idempotency_key!==originalKey);this.persist(data);}
}
