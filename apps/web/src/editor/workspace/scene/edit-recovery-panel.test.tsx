// @vitest-environment jsdom
import {cleanup,render,screen} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {EditRecoveryPanel} from "./edit-recovery-panel";
import type {EditRecoveryState} from "./edit-recovery";
const context=vi.hoisted(()=>({value:{} as unknown}));
vi.mock("../context",()=>({useWorkspace:()=>context.value,useEditingWorkspace:()=>context.value}));
afterEach(cleanup);
const recovery=(phase:EditRecoveryState["phase"],record=false):EditRecoveryState=>({phase,record:record?{context:{intent:{kind:"public_apply",request:{label:"Move clip"}}}} as never:null,receipt:phase==="saved"?{} as never:null,error:null});
it.each(["reading","staging","applying","saved"] as const)("does not ask to review an ordinary %s operation",phase=>{context.value={scene:{recovery:recovery(phase)},controller:{},busy:phase!=="saved"};render(<EditRecoveryPanel/>);expect(screen.queryByRole("region")).toBeNull();});
it("still exposes a previous unconfirmed request after the active save ends",()=>{context.value={scene:{recovery:recovery("unconfirmed",true)},controller:{},busy:false};render(<EditRecoveryPanel/>);expect(screen.getByRole("region")).toBeTruthy();});
