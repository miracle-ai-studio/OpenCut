import {afterEach,expect,it,vi} from "vitest";
import {bytesToHex} from "@noble/hashes/utils.js";
import {sha256} from "@noble/hashes/sha2.js";
import {loadCanvasFont,loadProjectFont} from "./canvas-font";
import type {EditorApi} from "../../api";
const bytes=new Uint8Array([1,2,3]),font={fontId:"font",contentHash:bytesToHex(sha256(bytes)),size:"3",weight:400,style:"normal" as const,faceIndex:0},scope={projectId:"project",revision:"fixed",sequenceId:"scene"};
afterEach(()=>vi.unstubAllGlobals());
function fixture(){
 const add=vi.fn(),remove=vi.fn(),load=vi.fn(async function(this:unknown){return this;});
 vi.stubGlobal("document",{fonts:{add,delete:remove}});vi.stubGlobal("FontFace",class{load(){return load.call(this);}});
 const call=vi.fn(async()=>({version:{content_hash:font.contentHash,size:3}})),downloadProjectResource=vi.fn(async()=>new Response(bytes));
 const api={connectionIdentity:"principal",sdk:{call,downloadProjectResource}} as unknown as EditorApi;
 return{api,call,downloadProjectResource,add,remove,load};
}
it("authorizes the exact fixed font resource, verifies bytes, and releases the view's font face",async()=>{
 const f=fixture(),face=await loadProjectFont(f.api,scope,font,new AbortController().signal);
 expect(f.call).toHaveBeenCalledWith("opencut_project_resource_inspect",{project_id:"project",revision:"fixed",kind:"font",resource_id:"font"},{signal:expect.any(AbortSignal)});
 expect(f.add).toHaveBeenCalledOnce();face.release();expect(f.remove).toHaveBeenCalledWith(f.add.mock.calls[0][0]);
});
it("rejects corrupted bytes and a connection replaced while a font face loads",async()=>{
 const f=fixture();f.downloadProjectResource.mockResolvedValueOnce(new Response(new Uint8Array([3,2,1])));
 await expect(loadProjectFont(f.api,scope,font,new AbortController().signal)).rejects.toThrow("identity changed");expect(f.add).not.toHaveBeenCalled();
 f.load.mockImplementationOnce(async()=>{Object.assign(f.api,{connectionIdentity:"replaced"});return {};});
 await expect(loadProjectFont(f.api,scope,font,new AbortController().signal)).rejects.toThrow("identity changed");expect(f.add).not.toHaveBeenCalled();
});
it("keeps the native glyph-coverage requirement on canvas editing",async()=>{
 const f=fixture();await expect(loadCanvasFont(f.api,scope,{...font,glyphCoverageValidated:false} as never,new AbortController().signal)).rejects.toThrow("Canvas text editing unavailable");expect(f.call).not.toHaveBeenCalled();
});
