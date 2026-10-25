import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { runHost as sharedHost } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { validateBegin, validateTextBegin, validateCaptionBegin, validateGraphicBegin, validateBlendBegin, validateMaskBegin, validateSceneBegin, validateAdmission } from "./protocol.mjs";
import { browserHooks } from "./browser.mjs";
export const runHost=(input=process.stdin,output=process.stdout)=>sharedHost(input,output,{validateBegin:packet=>packet.type==="beginSceneFrame"?validateSceneBegin(packet):packet.type==="beginMaskFrame"?validateMaskBegin(packet):packet.type==="beginBlendFrame"?validateBlendBegin(packet):packet.type==="beginGraphicFrame"?validateGraphicBegin(packet):packet.type==="beginCaptionFrame"?validateCaptionBegin(packet):packet.type==="beginTextFrame"?validateTextBegin(packet):validateBegin(packet),validateAdmission,browser:browserHooks,executionName:"visual"});
if(process.argv[1]&&await fs.realpath(process.argv[1]).catch(()=>undefined)===fileURLToPath(import.meta.url))await runHost();
