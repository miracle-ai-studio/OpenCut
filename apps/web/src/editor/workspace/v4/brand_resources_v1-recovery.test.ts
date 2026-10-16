import { expect, it } from "vitest";
import { BrandRecoveryV1 } from "./brand_resources_v1-recovery";
import type { BrandImportV1 } from "./brand_resources_v1";
const identity=JSON.stringify(["https://brands.example", "principal", "credential-id", false]);
const request:BrandImportV1={schema_version:1,project_id:"p",expected_revision:"r",idempotency_key:"original",catalog_id:"simple-icons:16.35.0:dotnet",asset_id:"brand",uploaded:{media_id:"upload",version:{id:`av_${"a".repeat(64)}`,content_hash:"a".repeat(64),size:100}}};
function fixture(){const values=new Map<string,string>();return{values,storage:{getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>{values.set(key,value);}}};}
it("retains an immutable original intent across reload and removes only its exact scope/key",()=>{
  const f=fixture(),recovery=new BrandRecoveryV1(f.storage);recovery.put(identity,request);
  request.uploaded.version.size=101;expect(new BrandRecoveryV1(f.storage).get(identity,"p")?.uploaded.version.size).toBe(100);request.uploaded.version.size=100;
  expect(()=>recovery.put(identity,{...request,expected_revision:"different"})).toThrow();
  recovery.remove(identity,"p","other");expect(recovery.get(identity,"p")).toEqual(request);
  recovery.remove(identity,"p","original");expect(recovery.get(identity,"p")).toBeNull();expect([...f.values.keys()]).toEqual(["opencut.brand-resources.v1"]);
});
it("rejects credential URLs, source URLs, extra payload and unpinned versions",()=>{
  const f=fixture(),recovery=new BrandRecoveryV1(f.storage);
  expect(()=>recovery.put(JSON.stringify(["https://user:secret@brands.example","p","c",false]),request)).toThrow();
  for(const changed of [{...request,catalog_id:"https://example.org/logo.svg"},{...request,catalog_id:"simple-icons:latest:dotnet"},{...request,svg:"<svg/>"},{...request,schema_version:2}]){
    f.values.set("opencut.brand-resources.v1",JSON.stringify({version:1,entries:[{identity,request:changed}]}));expect(()=>recovery.get(identity,"p")).toThrow();
  }
});
it("bounds the shared envelope and rejects duplicate or malformed immutable identities",()=>{
  const f=fixture(),recovery=new BrandRecoveryV1(f.storage);
  for(let i=0;i<16;i++)recovery.put(identity,{...request,project_id:`p${i}`});
  expect(()=>recovery.put(identity,{...request,project_id:"p16"})).toThrow();
  for(const entries of [[{identity,request},{identity,request}], [{identity,request:{...request,uploaded:{...request.uploaded,version:{...request.uploaded.version,id:"av_other"}}}}]]){
    f.values.set("opencut.brand-resources.v1",JSON.stringify({version:1,entries}));expect(()=>recovery.get(identity,"p")).toThrow();
  }
});
