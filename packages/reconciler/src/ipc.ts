/**
 * Guest-side IPC bridge (Phase 8).
 *
 * Mirrors the Rust `IpcEnvelope` tuple exactly: `[Version, Flags, Type, ID, Method, Payload]`.
 * Encoding uses `@msgpack/msgpack` so the byte layout matches `rmp-serde` on the host —
 * both are canonical MessagePack, so arrays/maps/ints round-trip identically.
 */

import { encode } from "@msgpack/msgpack";

export const IPC_VERSION = 1;

export const EnvelopeType = {
    Request: 0,
    Response: 1,
    Event: 2,
    UiSync: 3,
    Cancel: 4,
} as const;

export type EnvelopeTypeValue = (typeof EnvelopeType)[keyof typeof EnvelopeType];

export const IpcFlags = {
    NONE: 0,
    COMPRESSED: 0x01,
    ENCRYPTED: 0x02,
    STREAMED: 0x04,
    PARTIAL: 0x08,
    HIGH_PRIORITY: 0x10,
} as const;

/** A single Deno op binding, injected by the host via `#[op2]`. Absent capabilities are never registered. */
interface DenoCoreOps {
    op_ipc_post(payload: Uint8Array): void;
    op_ipc_recv(): Promise<Uint8Array>;
}

interface DenoCoreGlobal {
    ops: DenoCoreOps;
}

/** Narrow, validated accessor for the `Deno.core` bridge injected by the Rust host at isolate boot. */
export function getDenoCore(): DenoCoreGlobal {
    const candidate = (globalThis as Record<string, unknown>).Deno;
    if (
        candidate &&
        typeof candidate === "object" &&
        "core" in candidate &&
        candidate.core &&
        typeof candidate.core === "object" &&
        "ops" in candidate.core
    ) {
        return candidate.core as DenoCoreGlobal;
    }
    throw new Error("IpcError: Deno.core bridge is unavailable — is this running inside the SuperSearch sandbox?");
}

/** Builds and posts a MessagePack envelope of the given `type`. Shared by every post-side domain helper below. */
function postEnvelope(type: EnvelopeTypeValue, id: number, method: string, payload: unknown, flags: number): void {
    const envelope: [number, number, number, number, string, unknown] = [IPC_VERSION, flags, type, id, method, payload];
    getDenoCore().ops.op_ipc_post(encode(envelope));
}

/** Posts a UiSync envelope carrying the current UI node tree produced by the reconciler. */
export function postUiSync(payload: unknown, flags: number = IpcFlags.NONE): void {
    postEnvelope(EnvelopeType.UiSync, 0, "", payload, flags);
}

/** Posts a Request envelope, used by the SDK's system/network/storage modules to call host APIs. */
export function postRequest(id: number, method: string, payload: unknown): void {
    postEnvelope(EnvelopeType.Request, id, method, payload, IpcFlags.NONE);
}
