import { validateEnvelope } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { integer, natural, reject } from "../src/validation.mjs";
export function validateAdmission(value) {
  if(!value||!["monitored-terminate","strict-hard"].includes(value.quotaProfile))reject("INVALID_VISUAL_ADMISSION","Explicit quota profile is required");
  integer(value.deadlineMs,"deadline",1,300000);integer(value.maxEncodedBytes,"encoded budget",1,32*1024*1024);
  for(const key of ["maxFramePixels","maxDecodedPixels","maxGpuBytes","maxRgbaBytes","maxOutputBytes","maxRssBytes"]){const n=natural(value[key],key);if(n===0n||n>8589934592n)reject("INVALID_VISUAL_ADMISSION","Admission is outside host bounds");}
  if(natural(value.maxRssBytes,"RSS")<1048576n)reject("INVALID_VISUAL_ADMISSION","RSS admission is too small");
  return {...structuredClone(value),monitorIntervalMs:integer(value.monitorIntervalMs??100,"monitor interval",20,250)};
}
export function validateBegin(packet) {
  validateEnvelope(packet);if(packet.type!=="begin")reject("INVALID_HOST_PROTOCOL","First packet must begin a fixed frame");
  const begin=structuredClone(packet);begin.admission=validateAdmission(begin.admission);const plan=begin.plan;
  if(!plan||plan.identity?.profile!=="classic-v31-gpu-normal-frame"||!Array.isArray(plan.resources)||!Array.isArray(plan.layers)||plan.resources.length>32||plan.layers.length>60||!Array.isArray(begin.resources))reject("INVALID_HOST_PROTOCOL","Invalid bounded visual plan");
  for(const key of ["projectId","sequenceId","revision"])if(typeof plan.identity[key]!=="string"||!plan.identity[key])reject("INVALID_HOST_PROTOCOL","Missing fixed identity");
  integer(plan.identity.width,"width",1,16384);integer(plan.identity.height,"height",1,16384);
  const known=new Map();
  for(const resource of plan.resources) {
    if(typeof resource.assetId!=="string"||!resource.assetId||known.has(resource.assetId)||!/^[a-f0-9]{64}$/.test(resource.contentHash)||!["video","image"].includes(resource.kind)||natural(resource.size,"encoded size")===0n||natural(resource.size,"encoded size")>BigInt(begin.admission.maxEncodedBytes))reject("INVALID_HOST_PROTOCOL","Invalid visual CAS identity");
    known.set(resource.assetId,resource);
  }
  const handles=new Set();for(const resource of begin.resources){if(!known.has(resource.assetId)||handles.has(resource.assetId)||typeof resource.handle!=="string"||!resource.handle||resource.handle.length>1024)reject("INVALID_HOST_PROTOCOL","Invalid private visual handles");handles.add(resource.assetId);}
  if(handles.size!==known.size)reject("INVALID_HOST_PROTOCOL","Missing private resource handle");
  const elements=new Set();
  for(const layer of plan.layers) {
    if(typeof layer.elementId!=="string"||!layer.elementId||elements.has(layer.elementId)||JSON.stringify(known.get(layer.resource?.assetId))!==JSON.stringify(layer.resource)||!["image","video"].includes(layer.decode?.kind))reject("INVALID_HOST_PROTOCOL","Invalid fixed visual binding");
    elements.add(layer.elementId);
  }
  return begin;
}
