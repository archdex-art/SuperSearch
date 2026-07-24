/**
 * System interaction APIs (Phase 7).
 *
 * All operations here are securely capability-gated. If the extension lacks
 * the required capability in its manifest, the Rust host intercepts the request
 * and immediately rejects it with a `CapabilityError`.
 */

import { postRequest, awaitResponse } from "@supersearch/reconciler";

let nextRequestId = 1000; // Offset to avoid collision with system IDs

/**
 * Asynchronously invokes a host system command.
 */
async function invokeHost<T>(method: string, payload: unknown): Promise<T> {
    const id = ++nextRequestId;
    postRequest(id, method, payload);
    const result = await awaitResponse(id);
    return result as T;
}

export const Clipboard = {
    /**
     * Reads text from the OS clipboard.
     * Requires capability: `clipboard-read`
     */
    async readText(): Promise<string> {
        return invokeHost<string>("clipboard.readText", null);
    },

    /**
     * Copies text to the OS clipboard.
     * Requires capability: `clipboard-write`
     */
    async copy(text: string): Promise<void> {
        await invokeHost<void>("clipboard.writeText", { text });
    }
};

export async function open(url: string): Promise<void> {
    await invokeHost<void>("shell.open", { url });
}
