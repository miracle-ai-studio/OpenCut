// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { V4BrandCatalogPanel } from "./brand_resources_v1-panel";
import { BrandRecoveryV1 } from "./brand_resources_v1-recovery";
import { preferences } from "../../shared/locale";
import type { BrandEntryV1, BrandImportV1 } from "./brand_resources_v1";
import type { ToolOutput } from "../../../lib/editor-sdk";
import type { V4Asset } from "./types";
const mocks=vi.hoisted(()=>({state:{} as unknown,png:vi.fn(),upload:vi.fn()}));
vi.mock("../context",()=>({useWorkspace:()=>mocks.state}));
vi.mock("../upload",()=>({uploadMedia:mocks.upload}));
vi.mock("./brand_resources_v1",async(importOriginal)=>({...await importOriginal<typeof import("./brand_resources_v1")>(),brandPngV1:mocks.png}));
const identity=JSON.stringify(["https://brands.example","principal","credential-id",false]);
const version={id:`av_${"a".repeat(64)}`,content_hash:"a".repeat(64),size:100};
const entry:BrandEntryV1={id:"simple-icons:16.35.0:dotnet",name:".NET",slug:"dotnet",keywords:[],color:"512BD4",source_url:"https://github.com/dotnet/brand",guidelines_url:null,svg_sha256:"b".repeat(64),svg_size:100,license_identifier:"CC0-1.0",license_url:null,import_available:true,unavailable_reason:null};
const catalog:ToolOutput<"opencut_brand_resources_v1_list">={schema_version:1,collection:{name:"Simple Icons",release:"16.35.0",commit:"ed9a5192b24ad2f169bf1009931da0df27d5a49a",source_url:"https://github.com/simple-icons/simple-icons",inventory_sha256:"a".repeat(64),collection_license:{identifier:"CC0-1.0",source_url:null,notice:"collection only",attribution:null},disclaimer:"Read individual licenses"},total:3466,matched:3466,offset:0,next_offset:40,entries:[entry,{...entry,id:"simple-icons:16.35.0:unknown",name:"Unknown Brand",slug:"unknown",import_available:false,license_identifier:null,unavailable_reason:"No per-icon license is recorded."}]};
beforeEach(()=>{preferences({language:"en"});sessionStorage.clear();mocks.png.mockReset().mockResolvedValue(new File(["png"],"brand.png",{type:"image/png"}));mocks.upload.mockReset().mockResolvedValue({media_id:"uploaded",version});});
afterEach(cleanup);
function fixture(){
  const call=vi.fn(async(tool:string)=>tool==="opencut_brand_resources_v1_list"?catalog:{entry});
  const imported=vi.fn(async(request:BrandImportV1)=>({transaction_id:"tx",base_revision:request.expected_revision,revision:"r2",created_ids:[request.asset_id],changed_ids:[],deleted_ids:[]}));
  const state={api:{connectionIdentity:identity,authenticateConnection:async()=>{},sdk:{call}},v4:{head:{project:{id:"p",assets:[] as V4Asset[]},revision:"r"},media:{attempt:null}},busy:false,controller:{getSnapshot:()=>state,report:vi.fn(),v4:{importBrandResource:imported,refresh:vi.fn(async()=>{
    const request=imported.mock.calls.at(-1)?.[0];if(!request)return;
    state.v4.head={revision:"r2",project:{id:"p",assets:[{id:request.asset_id,name:".NET",kind:"image",version,duration:null,origin:{kind:"generated",provider:"simple-icons-browser-raster-v1",generation_id:"fixed-source"},license:{identifier:"CC0-1.0",source_url:null,notice:"full notice",attribution:"source attribution"},metadata:{mime_type:"image/png",intrinsic_size:{width:512,height:512},frame_rate:null,rotation_degrees:null,audio:{kind:"absent"}}}]}};
  })}}};
  mocks.state=state;return{state,call,imported};
}
it("shows the complete offline inventory count, individual license blocking and bounded pagination",async()=>{
  const f=fixture();render(<V4BrandCatalogPanel query=""/>);
  expect(await screen.findByText(/3466 \/ 3466/)).toBeTruthy();
  expect((screen.getByRole("button",{name:"Acquire brand · Unknown Brand"}) as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByRole("button",{name:"Acquire brand · .NET"}) as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(screen.getByRole("button",{name:"Next"}));
  await waitFor(()=>expect(f.call).toHaveBeenCalledWith("opencut_brand_resources_v1_list",{query:null,offset:40,limit:40},expect.anything()));
  expect(f.imported).not.toHaveBeenCalled();expect(mocks.upload).not.toHaveBeenCalled();
});
it("a lost import response retries the stored original request without rasterizing or uploading again",async()=>{
  const f=fixture();f.imported.mockRejectedValueOnce(Error("response lost"));render(<V4BrandCatalogPanel query=""/>);
  fireEvent.click(await screen.findByRole("button",{name:"Acquire brand · .NET"}));
  await screen.findByText("response lost");const original=new BrandRecoveryV1().get(identity,"p");expect(original?.catalog_id).toBe(entry.id);
  fireEvent.click(screen.getByRole("button",{name:"Retry original brand acquisition"}));
  await waitFor(()=>expect(new BrandRecoveryV1().get(identity,"p")).toBeNull());
  expect(f.imported).toHaveBeenCalledTimes(2);expect(f.imported.mock.calls[0][0]).toEqual(f.imported.mock.calls[1][0]);expect(mocks.png).toHaveBeenCalledTimes(1);expect(mocks.upload).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button",{name:"Download license and attribution"})).toBeTruthy();
});
it("reopens an unresolved original request and never turns it into a new upload or new head",async()=>{
  const f=fixture(),request:BrandImportV1={schema_version:1,project_id:"p",expected_revision:"r",idempotency_key:"saved-original",catalog_id:entry.id,asset_id:"saved-brand",uploaded:{media_id:"uploaded",version}};
  new BrandRecoveryV1().put(identity,request);f.state.v4.head.revision="later";render(<V4BrandCatalogPanel query=".NET"/>);
  fireEvent.click(await screen.findByRole("button",{name:"Retry original brand acquisition"}));
  await waitFor(()=>expect(f.imported).toHaveBeenCalledWith(request,expect.any(AbortSignal)));expect(mocks.png).not.toHaveBeenCalled();expect(mocks.upload).not.toHaveBeenCalled();
});
it("a source response arriving after the brand view closes cannot upload, publish or retain a new intent",async()=>{
  const f=fixture();let resolve!:(value:{entry:BrandEntryV1})=>void;const pending=new Promise<{entry:BrandEntryV1}>(r=>{resolve=r;});
  f.call.mockImplementation((tool:string)=>tool==="opencut_brand_resources_v1_source"?pending:Promise.resolve(catalog));
  const view=render(<V4BrandCatalogPanel query=""/>);fireEvent.click(await screen.findByRole("button",{name:"Acquire brand · .NET"}));
  await waitFor(()=>expect(f.call.mock.calls.some(([tool])=>tool==="opencut_brand_resources_v1_source")).toBe(true));
  view.unmount();await act(async()=>{resolve({entry});await pending;});
  expect(mocks.png).not.toHaveBeenCalled();expect(mocks.upload).not.toHaveBeenCalled();expect(f.imported).not.toHaveBeenCalled();expect(new BrandRecoveryV1().get(identity,"p")).toBeNull();
});
