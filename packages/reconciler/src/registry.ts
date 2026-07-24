/**
 * Callback Registry (Phase 5)
 *
 * Maps JavaScript closures (which cannot be serialized over IPC) to unique string IDs.
 * The Rust host returns these IDs when native events occur (e.g., a user clicks a button),
 * allowing this registry to trigger the original React closure.
 */

let nextId = 0;
const registry = new Map<string, Function>();

/**
 * Strips functions from a props object, generating a CallbackID for each.
 * Returns a tuple of [SafeProps, EventMap].
 */
export function extractEvents(props: Record<string, unknown>): [Record<string, unknown>, Record<string, string>] {
    const safeProps: Record<string, unknown> = {};
    const events: Record<string, string> = {};

    for (const [key, value] of Object.entries(props)) {
        if (typeof value === "function") {
            const id = `cb_${++nextId}`;
            registry.set(id, value);
            events[key] = id;
        } else if (key !== "children") {
            safeProps[key] = value;
        }
    }

    return [safeProps, events];
}

/**
 * Removes old callbacks to prevent memory leaks when components re-render or unmount.
 */
export function garbageCollectEvents(events: Record<string, string>) {
    for (const id of Object.values(events)) {
        registry.delete(id);
    }
}

/**
 * Triggers a stored callback based on its ID.
 * Bound to the `op_ipc_recv` listener in the guest bootstrap.
 */
export function dispatchEvent(id: string, payload: unknown) {
    const cb = registry.get(id);
    if (cb) {
        cb(payload);
    } else {
        console.warn(`[Reconciler] Attempted to dispatch unregistered event: ${id}`);
    }
}
