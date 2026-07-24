/**
 * Guest event loop (Phase 8 §5 — Action Dispatch & Event Synchronization).
 *
 * Continuously awaits `op_ipc_recv`, which suspends the isolate's microtask queue
 * until the Rust host pushes a Host → Guest envelope (an `Event`, or a `Response`
 * to a pending `Request`). Each envelope is routed to its destination:
 *
 * - `Event`    → dispatched by `CallbackID` through the registry (native UI actions).
 * - `Response` → resolves the pending promise registered under the matching request ID.
 */

import { decode } from "@msgpack/msgpack";
import { EnvelopeType, getDenoCore } from "./ipc";
import { dispatchEvent } from "./registry";

type IpcEnvelopeTuple = [number, number, number, number, string, unknown];

const pendingRequests = new Map<number, (payload: unknown) => void>();

/** Registers a resolver for a Request awaiting its matching Response envelope by ID. */
export function awaitResponse(id: number): Promise<unknown> {
    const { promise, resolve } = Promise.withResolvers<unknown>();
    pendingRequests.set(id, resolve);
    return promise;
}

function routeEnvelope(envelope: IpcEnvelopeTuple): void {
    const [, , type, id, method, payload] = envelope;

    if (type === EnvelopeType.Event) {
        dispatchEvent(method, payload);
        return;
    }

    if (type === EnvelopeType.Response) {
        const resolve = pendingRequests.get(id);
        if (resolve) {
            pendingRequests.delete(id);
            resolve(payload);
        }
    }
}

/**
 * Starts the guest's inbound message pump. Must be called exactly once during isolate
 * bootstrap; it recurses via `.finally` rather than a `while (true)` loop so a single
 * malformed envelope logs and continues instead of unwinding the whole pump.
 */
export function startEventLoop(): void {
    const recv = getDenoCore().ops.op_ipc_recv;

    const pump = (): void => {
        recv()
            .then((bytes) => routeEnvelope(decode(bytes) as IpcEnvelopeTuple))
            .catch((error: unknown) => {
                console.error("[EventLoop] Failed to process inbound envelope:", error);
            })
            .finally(pump);
    };

    pump();
}
