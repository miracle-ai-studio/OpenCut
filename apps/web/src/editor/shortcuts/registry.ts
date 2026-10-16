import { ACTIONS, type ShortcutActionId } from "./catalog";

export type ShortcutHandler = {
  run(): unknown | Promise<unknown>;
  enabled?: boolean;
  reason?: string;
  checked?: boolean;
  /** Default Mod+V reaches the native paste event to retain File objects.
   * Custom keys call run(), which uses the same clipboard controller. */
  nativePaste?: boolean;
  /** Held view tools must release on keyup, blur, page departure and teardown. */
  release?(): void;
};
export type ShortcutHandlers = Partial<
  Record<ShortcutActionId, ShortcutHandler>
>;
export type ShortcutAvailability = {
  enabled: boolean;
  reason: string | null;
  checked?: boolean;
};

/** Registrations contain closures only, never copies of project state. Local
 * interaction owners may override the broader cancel/selection action. */
export class ShortcutRegistry {
  private owners = new Map<
    symbol,
    { read(): ShortcutHandlers; priority: number }
  >();
  private listeners = new Set<() => void>();
  private inFlight = new Set<ShortcutActionId>();
  private version = 0;
  private signature = "";
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  snapshot = () => this.version;
  register(owner: symbol, actions: () => ShortcutHandlers, priority = 0) {
    this.owners.set(owner, { read: actions, priority });
    this.refresh();
    return () => {
      this.owners.delete(owner);
      this.refresh();
    };
  }
  handler(id: ShortcutActionId): ShortcutHandler | undefined {
    let found: ShortcutHandler | undefined;
    let priority = -Infinity;
    for (const owner of this.owners.values()) {
      const candidate = owner.read()[id];
      if (candidate && owner.priority >= priority) {
        found = candidate;
        priority = owner.priority;
      }
    }
    return found;
  }
  availability(id: ShortcutActionId): ShortcutAvailability {
    const handler = this.handler(id);
    return !handler
      ? {
          enabled: false,
          reason: "This action is not available in the current editor view.",
        }
      : this.inFlight.has(id)
        ? {
            enabled: false,
            reason: "This action is still in progress.",
            checked: handler.checked,
          }
        : {
            enabled: handler.enabled !== false,
            reason:
              handler.enabled === false
                ? (handler.reason ?? "This action is currently unavailable.")
                : null,
            checked: handler.checked,
          };
  }
  refresh() {
    const signature = JSON.stringify(
      ACTIONS.map((action) => this.availability(action.id)),
    );
    if (signature === this.signature) return;
    this.signature = signature;
    this.version++;
    this.listeners.forEach((listener) => listener());
  }
  async invoke(id: ShortcutActionId): Promise<boolean> {
    const handler = this.handler(id);
    if (!handler || !this.availability(id).enabled) return false;
    this.inFlight.add(id);
    this.refresh();
    try {
      await handler.run();
      return true;
    } finally {
      this.inFlight.delete(id);
      this.refresh();
    }
  }
}
