import type { DerivedFontImport } from "./font-derived";
import { verifyDerivedFontImport } from "./font-derived";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { V4SessionHost } from "../session";
import { exactRecovery } from "../edit-recovery";
import { RecoveryPointersV2 } from "../edit-recovery-pointers";

export type FontRecoveryContext =
  ToolInput<"opencut_font_recovery_v1_stage">["context"];
export type FontRecoveryRecord = NonNullable<
  ToolOutput<"opencut_font_recovery_v1_inspect">["record"]
>;
type FontProtocol = "static" | "derived";
const protocolTools = (protocol: FontProtocol, catalogId = "") =>
  protocol === "derived"
    ? ({
        read: "opencut_font_derived_v1_recovery_read",
        stage: "opencut_font_derived_v1_recovery_stage",
        ack: "opencut_font_derived_v1_recovery_ack",
        import: catalogId.endsWith("-synthetic-italic-v1")
          ? "opencut_font_synthesis_v1_import"
          : catalogId.endsWith("-derived-v2")
            ? "opencut_font_derived_v2_import"
            : "opencut_font_derived_v1_import",
      } as const)
    : ({
        read: "opencut_font_recovery_v1_inspect",
        stage: "opencut_font_recovery_v1_stage",
        ack: "opencut_font_recovery_v1_ack",
        import: catalogId.endsWith("-v3")
          ? "opencut_font_import_v3"
          : "opencut_font_v1_import",
      } as const);
type Receipt = NonNullable<FontRecoveryRecord["receipt"]>;
export type FontRecoveryState = {
  supported: boolean | null;
  protocol?: FontProtocol;
  derivedRequest?: DerivedFontImport;
  derivedSupported?: boolean;
  staticSupported?: boolean;
  staticV3Supported?: boolean;
  derivedV2Supported?: boolean;
  synthesisSupported?: boolean;
  phase:
    | "idle"
    | "reading"
    | "staging"
    | "applying"
    | "unconfirmed"
    | "saved"
    | "failed";
  record: FontRecoveryRecord | null;
  receipt: Receipt | null;
  error: string | null;
};
export const emptyFontRecovery = (): FontRecoveryState => ({
  supported: null,
  phase: "idle",
  record: null,
  receipt: null,
  error: null,
});
export const fontRecoveryBlocked = (state?: FontRecoveryState) =>
  !!state &&
  (!!state.record ||
    !!state.error ||
    ["reading", "staging", "applying"].includes(state.phase));
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
function fixed<T>(value: T): T {
  const copy = structuredClone(value);
  const freeze = (item: unknown) => {
    if (item && typeof item === "object" && !Object.isFrozen(item)) {
      Object.values(item).forEach(freeze);
      Object.freeze(item);
    }
  };
  freeze(copy);
  return copy;
}
function assertReceipt(context: FontRecoveryContext, receipt: Receipt) {
  if (
    receipt.base_revision !== context.request.expected_revision ||
    !receipt.transaction_id ||
    !receipt.revision
  )
    throw new Error("The font receipt differs from the original import.");
}
type Fence = {
  api: EditorApi;
  identity: string;
  projectId: string;
  generation: number;
  view: number;
  serial: number;
};

/** The host owns the durable intent. Only an explicit retry can import again;
 * discovery never mistakes a new project head for an import receipt. */
export class V4FontRecovery {
  private serial = 0;
  private snapshot: {
    identity: string;
    projectId: string;
    value: FontRecoveryState;
  } | null = null;
  // Font and edit recovery share one host slot and the same non-secret locator.
  private readonly pointers = new RecoveryPointersV2();
  constructor(
    private readonly host: V4SessionHost,
    private readonly refresh: () => Promise<void>,
  ) {}
  private fence(api: EditorApi, projectId: string): Fence {
    if (!api.connectionIdentity)
      throw new Error("Authenticate the connection before editing.");
    return {
      api,
      identity: api.connectionIdentity,
      projectId,
      generation: this.host.generation(),
      view: this.host.viewGeneration(),
      serial: this.serial,
    };
  }
  private current(fence: Fence) {
    return (
      fence.api === this.host.api() &&
      fence.identity === fence.api.connectionIdentity &&
      fence.serial === this.serial &&
      fence.generation === this.host.generation() &&
      fence.view === this.host.viewGeneration()
    );
  }
  private check(fence: Fence) {
    if (!this.current(fence))
      throw new DOMException("The edit target changed.", "AbortError");
  }
  private value(fence: Fence) {
    return this.snapshot?.identity === fence.identity &&
      this.snapshot.projectId === fence.projectId
      ? this.snapshot.value
      : emptyFontRecovery();
  }
  private publish(fence: Fence, value: FontRecoveryState) {
    if (!this.current(fence)) return;
    this.snapshot = {
      identity: fence.identity,
      projectId: fence.projectId,
      value,
    };
    const state = this.host.state();
    if (state?.head.project.id === fence.projectId)
      this.host.publish({ ...state, fontRecovery: value });
  }
  invalidate() {
    this.serial++;
  }
  resetPointer() {
    this.pointers.reset();
  }
  async restore(api: EditorApi, projectId: string): Promise<FontRecoveryState> {
    this.serial++;
    const fence = this.fence(api, projectId),
      previous = this.value(fence);
    this.publish(fence, { ...previous, phase: "reading", error: null });
    try {
      const capabilities = await api.sdk.hostCapabilities();
      this.check(fence);
      if (!Array.isArray(capabilities.tools))
        throw new Error("The service returned invalid tool discovery.");
      const staticSupported = capabilities.tools.includes(
          "opencut_font_recovery_v1_inspect",
        ),
        derivedSupported = capabilities.tools.includes(
          "opencut_font_derived_v1_recovery_read",
        );
      if (!staticSupported && !derivedSupported) {
        if (previous.record || previous.receipt)
          throw new Error(
            "The service cannot inspect the original font import.",
          );
        const value = { ...emptyFontRecovery(), supported: false };
        this.publish(fence, value);
        return value;
      }
      let protocol: FontProtocol = "static";
      let result = staticSupported
        ? await api.sdk.call("opencut_font_recovery_v1_inspect", {
            project_id: projectId,
            recovery_id: null,
          })
        : { schema_version: 1, record: null };
      this.check(fence);
      if (!result.record && derivedSupported) {
        protocol = "derived";
        result = await api.sdk.call("opencut_font_derived_v1_recovery_read", {
          project_id: projectId,
          recovery_id: null,
        });
        this.check(fence);
      }
      const record = result.record;
      if (
        result.schema_version !== 1 ||
        (record &&
          (record.schema_version !== 1 ||
            record.project_id !== projectId ||
            record.context.request.project_id !== projectId ||
            !record.recovery_id ||
            !record.request_digest))
      )
        throw new Error(
          "The service returned a different original font import.",
        );
      if (!record && previous.record && !previous.receipt)
        throw new Error("The original font import outcome remains unknown.");
      if (
        record &&
        previous.record &&
        (record.recovery_id !== previous.record.recovery_id ||
          record.request_digest !== previous.record.request_digest ||
          exactRecovery(record.context) !==
            exactRecovery(previous.record.context) ||
          protocol !== (previous.protocol ?? "static"))
      )
        throw new Error(
          "The service returned a different original font import.",
        );
      if (!record && previous.protocol === "derived" && previous.receipt) {
        const request =
          previous.derivedRequest ?? previous.record?.context.request;
        if (!request)
          throw new Error("The original font import outcome remains unknown.");
        await verifyDerivedFontImport(api, request, previous.receipt.revision);
        this.check(fence);
      }
      if (record?.receipt) assertReceipt(record.context, record.receipt);
      if (record)
        this.pointers.put(fence.identity, projectId, record.recovery_id);
      // A null font discovery may represent an existing V1/V2 edit. Do not
      // remove that family's shared locator or acknowledge its record.
      const value: FontRecoveryState = {
        supported: true,
        protocol: record ? protocol : previous.protocol,
        derivedRequest:
          record && protocol === "derived"
            ? fixed(record.context.request)
            : previous.derivedRequest,
        derivedSupported,
        staticSupported,
        staticV3Supported: capabilities.tools.includes(
          "opencut_font_import_v3",
        ),
        derivedV2Supported: capabilities.tools.includes(
          "opencut_font_derived_v2_import",
        ),
        synthesisSupported:
          capabilities.tools.includes("opencut_font_synthesis_v1_import") &&
          capabilities.tools.includes("opencut_font_synthesis_v1_proof"),
        record: record ? fixed(record) : null,
        receipt: record ? (record.receipt ?? null) : previous.receipt,
        phase: record
          ? record.receipt
            ? "saved"
            : "unconfirmed"
          : previous.receipt
            ? "saved"
            : "idle",
        error: null,
      };
      this.publish(fence, value);
      return value;
    } catch (error) {
      if (!this.current(fence)) return this.value(fence);
      const value: FontRecoveryState = {
        ...previous,
        phase: previous.receipt
          ? "saved"
          : previous.record
            ? "unconfirmed"
            : "failed",
        error: message(error),
      };
      this.publish(fence, value);
      return value;
    }
  }
  private async acknowledge(
    fence: Fence,
    record: FontRecoveryRecord,
    known: Receipt | null,
  ) {
    this.check(fence);
    const protocol = this.value(fence).protocol ?? "static",
      tools = protocolTools(protocol);
    let proof;
    try {
      if (protocol === "derived" && known) {
        await verifyDerivedFontImport(
          fence.api,
          record.context.request,
          known.revision,
        );
        this.check(fence);
      }
      proof = await fence.api.sdk.call(tools.ack, {
        project_id: fence.projectId,
        recovery_id: record.recovery_id,
      });
    } catch (error) {
      if (
        !known ||
        !(error instanceof OpenCutApiError) ||
        error.code !== "NOT_FOUND"
      )
        throw error;
      const observed = await fence.api.sdk.call(tools.read, {
        project_id: fence.projectId,
        recovery_id: record.recovery_id,
      });
      this.check(fence);
      if (observed.schema_version !== 1 || observed.record !== null)
        throw error;
      this.pointers.remove(fence.identity, fence.projectId, record.recovery_id);
      return known;
    }
    this.check(fence);
    if (
      proof.schema_version !== 1 ||
      proof.recovery_id !== record.recovery_id ||
      (proof.resolution === "saved"
        ? !proof.receipt
        : proof.receipt !== null) ||
      (known && exactRecovery(known) !== exactRecovery(proof.receipt))
    )
      throw new Error(
        "The font acknowledgment differs from the original receipt.",
      );
    if (proof.receipt) {
      assertReceipt(record.context, proof.receipt);
      if (protocol === "derived" && !known) {
        this.publish(fence, {
          ...this.value(fence),
          protocol,
          derivedRequest: fixed(record.context.request),
          record,
          receipt: proof.receipt,
          phase: "saved",
          error: null,
        });
        await verifyDerivedFontImport(
          fence.api,
          record.context.request,
          proof.receipt.revision,
        );
        this.check(fence);
      }
    }
    this.pointers.remove(fence.identity, fence.projectId, record.recovery_id);
    return proof.receipt ?? null;
  }
  async execute(
    api: EditorApi,
    context: FontRecoveryContext,
    requestedProtocol: FontProtocol | "static_v3" | "synthesis" = "static",
  ): Promise<Receipt> {
    const protocol: FontProtocol =
      requestedProtocol === "static_v3"
        ? "static"
        : requestedProtocol === "synthesis"
          ? "derived"
          : requestedProtocol;
    const captured = fixed(context),
      projectId = captured.request.project_id,
      fence = this.fence(api, projectId),
      previous = this.value(fence);
    const tools = protocolTools(protocol, captured.request.catalog_id),
      derivedRequest = protocol === "derived" ? captured.request : undefined;
    const edit = this.host.state()?.recovery,
      legacy = this.host.state()?.timelineEdit.attempt;
    if (
      edit?.record ||
      edit?.error ||
      (edit && ["reading", "staging", "applying"].includes(edit.phase)) ||
      legacy?.recoveryId
    )
      throw new Error(
        "Resolve the original edit before starting another operation.",
      );
    if (
      previous.supported !== true ||
      (tools.import === "opencut_font_import_v3" &&
        previous.staticV3Supported !== true) ||
      (tools.import === "opencut_font_derived_v2_import" &&
        previous.derivedV2Supported !== true) ||
      (tools.import === "opencut_font_synthesis_v1_import" &&
        previous.synthesisSupported !== true) ||
      (protocol === "derived" && previous.derivedSupported !== true) ||
      (protocol === "static" && previous.staticSupported === false) ||
      (previous.record && (previous.protocol ?? "static") !== protocol) ||
      ["reading", "staging", "applying"].includes(previous.phase) ||
      (previous.record &&
        exactRecovery(previous.record.context) !== exactRecovery(captured)) ||
      (!previous.record && previous.error)
    )
      throw new Error(
        "Resolve the original font import before starting another operation.",
      );
    let record = previous.record,
      receipt = record ? previous.receipt : null;
    try {
      this.publish(fence, {
        ...previous,
        protocol,
        derivedRequest,
        record,
        receipt,
        phase: "staging",
        error: null,
      });
      const staged = await api.sdk.call(tools.stage, {
        context: captured,
      });
      this.check(fence);
      const observed = await api.sdk.call(tools.read, {
        project_id: projectId,
        recovery_id: staged.recovery_id,
      });
      this.check(fence);
      const candidate = observed.record;
      if (
        observed.schema_version !== 1 ||
        !candidate ||
        candidate.schema_version !== 1 ||
        candidate.recovery_id !== staged.recovery_id ||
        candidate.project_id !== projectId ||
        exactRecovery(candidate.context) !== exactRecovery(captured) ||
        !candidate.request_digest ||
        candidate.request_digest !== staged.request_digest
      )
        throw new Error(
          "The staged font import differs from the original request.",
        );
      if (candidate.receipt) assertReceipt(captured, candidate.receipt);
      record = fixed(candidate);
      receipt = candidate.receipt ?? null;
      this.publish(fence, {
        ...previous,
        supported: true,
        protocol,
        derivedRequest,
        record,
        receipt,
        phase: receipt ? "saved" : "unconfirmed",
        error: null,
      });
      this.pointers.put(fence.identity, projectId, record.recovery_id);
      this.check(fence);
      if (!receipt) {
        this.publish(fence, {
          ...previous,
          supported: true,
          protocol,
          derivedRequest,
          record,
          receipt: null,
          phase: "applying",
          error: null,
        });
        const result = await api.sdk.call(tools.import, captured.request);
        assertReceipt(captured, result);
        receipt = result;
      }
      this.publish(fence, {
        ...previous,
        supported: true,
        protocol,
        derivedRequest,
        record,
        receipt,
        phase: "saved",
        error: null,
      });
      if (this.current(fence)) {
        try {
          await this.acknowledge(fence, record, receipt);
          this.publish(fence, {
            ...previous,
            supported: true,
            protocol,
            derivedRequest,
            record: null,
            receipt,
            phase: "saved",
            error: null,
          });
        } catch (error) {
          this.publish(fence, {
            ...previous,
            supported: true,
            protocol,
            derivedRequest,
            record,
            receipt,
            phase: "saved",
            error: message(error),
          });
        }
      }
      return receipt;
    } catch (error) {
      this.publish(fence, {
        ...previous,
        supported: true,
        protocol,
        derivedRequest,
        record,
        receipt,
        phase: receipt ? "saved" : record ? "unconfirmed" : "failed",
        error: message(error),
      });
      throw error;
    }
  }
  async retry() {
    const api = this.host.api(),
      state = this.host.state(),
      record = state?.fontRecovery?.record;
    if (!api || !state || !record || state.historical)
      throw new Error(
        "Open the original current project to recover this font import.",
      );
    const release = this.host.acquire(record.project_id);
    try {
      const receipt = await this.execute(
        api,
        record.context,
        state.fontRecovery?.protocol ?? "static",
      );
      if (
        api === this.host.api() &&
        this.host.state()?.head.project.id === record.project_id
      )
        await this.refresh();
      return receipt;
    } finally {
      release();
    }
  }
  async release() {
    const api = this.host.api(),
      state = this.host.state(),
      record = state?.fontRecovery?.record;
    if (!api || !state || !record)
      throw new Error("No original font import needs recovery.");
    const fence = this.fence(api, record.project_id),
      release = this.host.acquire(record.project_id);
    try {
      const receipt = await this.acknowledge(
        fence,
        record,
        state.fontRecovery!.receipt,
      );
      this.publish(fence, {
        ...emptyFontRecovery(),
        protocol: this.value(fence).protocol,
        derivedRequest: this.value(fence).derivedRequest,
        derivedSupported: state.fontRecovery?.derivedSupported,
        staticSupported: state.fontRecovery?.staticSupported,
        staticV3Supported: state.fontRecovery?.staticV3Supported,
        derivedV2Supported: state.fontRecovery?.derivedV2Supported,
        supported: true,
        phase: receipt ? "saved" : "idle",
        receipt,
      });
      if (receipt) await this.refresh();
    } catch (error) {
      this.publish(fence, { ...this.value(fence), error: message(error) });
      throw error;
    } finally {
      release();
    }
  }
  dismissSaved() {
    const api = this.host.api(),
      state = this.host.state();
    if (
      !api ||
      !state ||
      state.fontRecovery?.record ||
      !state.fontRecovery?.receipt
    )
      throw new Error("Resolve the original font import before dismissing it.");
    this.publish(this.fence(api, state.head.project.id), {
      ...emptyFontRecovery(),
      derivedSupported: state.fontRecovery?.derivedSupported,
      staticSupported: state.fontRecovery?.staticSupported,
      staticV3Supported: state.fontRecovery?.staticV3Supported,
      derivedV2Supported: state.fontRecovery?.derivedV2Supported,
      supported: true,
    });
  }
}
