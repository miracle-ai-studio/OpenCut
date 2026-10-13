import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { runHost as sharedHost } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { validateBegin, validateTextBegin, validateCaptionBeginV3, validateAdmission } from "./protocol.mjs";
import { browserHooks } from "./browser.mjs";
export const runHost=(input=process.stdin,output=process.stdout)=>sharedHost(input,output,{validateBegin:packet=>packet.type==="beginCaptionFrameV3"?validateCaptionBeginV3(packet):packet.type==="beginTextFrameV2"?validateTextBegin(packet):validateBegin(packet),validateAdmission,browser:browserHooks,executionName:"visual"});
if(process.argv[1]&&await fs.realpath(process.argv[1]).catch(()=>undefined)===fileURLToPath(import.meta.url))await runHost();
