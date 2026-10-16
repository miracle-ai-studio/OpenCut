import { expect,it,vi } from "vitest";
import { HistoryPackageRecoveryV1 } from "./history-package-recovery";
import { verifyHistoryPackageReceipt, validateHistoryPackage } from "./history-package-model";
const identity=JSON.stringify(["http://localhost:18400","local-owner",null,true]);
const inspection={manifest_sha256:"a".repeat(64),source_project_id:"source",source_revision:"rev_fixed",project_name:"History",revision_count:4,transaction_count:3,branch_count:1,merge_count:0,asset_version_count:2,font_version_count:1,derived_font_count:0,self_contained_resources:true,history_included:true,fully_reproducible:false};
const value=()=>({request:{package_path:"/private/tmp/reviewed-package",target_project_id:"target",expected_manifest_sha256:inspection.manifest_sha256,idempotency_key:"original-key"},inspection:structuredClone(inspection)});
function fixture(){let raw:string|null=null;return {storage:{getItem:()=>raw,setItem:(_key:string,next:string)=>{raw=next;}},raw:(next:string)=>{raw=next;}};}
it("retains the exact original restore across remount and separates authenticated connections",()=>{
  const f=fixture(),first=new HistoryPackageRecoveryV1(f.storage),original=value();first.write(identity,original);
  const reopened=new HistoryPackageRecoveryV1(f.storage);expect(reopened.read(identity)).toEqual(original);
  expect(reopened.read(JSON.stringify(["http://localhost:18401","local-owner",null,true]))).toBeNull();
  expect(()=>reopened.write(identity,{...original,request:{...original.request,idempotency_key:"new-key"}})).toThrow();
  reopened.remove(identity,"another-key");expect(reopened.read(identity)).toEqual(original);
  reopened.remove(identity,"original-key");expect(reopened.read(identity)).toBeNull();
});
it("rejects credentials, unbounded paths, oversized envelopes and silent storage loss before dispatch",()=>{
  const f=fixture(),recovery=new HistoryPackageRecoveryV1(f.storage);
  expect(()=>recovery.write("bearer secret",value())).toThrow();
  expect(()=>recovery.write(identity,{...value(),request:{...value().request,package_path:"/"+"x".repeat(4096)}})).toThrow();
  expect(()=>new HistoryPackageRecoveryV1({getItem:()=>null,setItem:vi.fn()}).write(identity,value())).toThrow();
  f.raw(" ".repeat(65537));expect(()=>recovery.read(identity)).toThrow();
});
it("rejects a forged review or a receipt with lost history before displaying restored content",()=>{
  const original=value(),receipt={target_project_id:"target",revision:"rev_restored",package:inspection};
  verifyHistoryPackageReceipt(original.request,inspection,receipt);
  for(const changed of [{...inspection,history_included:false},{...inspection,history_included:"true"},{...inspection,revision_count:0},{...inspection,fully_reproducible:true}])expect(()=>validateHistoryPackage(changed as typeof inspection)).toThrow();
  expect(()=>verifyHistoryPackageReceipt(original.request,inspection,{...receipt,package:{...inspection,transaction_count:2}})).toThrow();
  expect(()=>verifyHistoryPackageReceipt(original.request,inspection,{...receipt,target_project_id:"other"})).toThrow();
});
