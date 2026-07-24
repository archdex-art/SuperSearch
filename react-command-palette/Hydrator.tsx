/**
 * Host Native Hydrator (Phase 5 & 6)
 *
 * This component runs inside the Tauri WebView (the main SuperSearch app).
 * It listens for `UiSync` envelopes coming over the IPC bridge from the Rust Host,
 * deserializes the MessagePack `UINode` tree, and maps those abstract nodes into
 * concrete Tailwind-styled React primitives.
 */

import React, { useEffect, useState } from "react";
import { invoke, listen } from "./bridge";

interface UINode {
    id: string;
    type: string;
    props: Record<string, unknown>;
    events: Record<string, string>;
    children: UINode[];
    text?: string;
}

/**
 * Dispatches an event back to the V8 guest.
 * Maps native DOM events (e.g. onClick) into `execute_extension_action` IPC calls
 * passing the assigned `CallbackID`.
 */
function dispatchToGuest(callbackId: string, payload?: unknown) {
    invoke("execute_extension_action", {
        action: { callbackId, payload }
    }).catch(console.error);
}

/**
 * Binds the serialized event map to actual React callbacks.
 */
function bindEvents(events: Record<string, string>): Record<string, Function> {
    const bound: Record<string, Function> = {};
    for (const [eventName, callbackId] of Object.entries(events)) {
        bound[eventName] = (payload: unknown) => dispatchToGuest(callbackId, payload);
    }
    return bound;
}

/**
 * Recursively maps abstract UINodes to native React components.
 */
function renderNode(node: UINode): React.ReactNode {
    if (node.type === "TEXT") {
        return node.text;
    }

    // Combine safe props from the guest with the bound native closures
    const boundEvents = bindEvents(node.events);
    const props = { ...node.props, ...boundEvents, key: node.id };

    const children = node.children.map(renderNode);

    // Map the abstract `type` string to our native host implementations.
    // In a real app, these map to robust polished components (e.g., cmdk primitives).
    switch (node.type) {
        case "List":
            return <div className="supersearch-list flex flex-col w-full h-full overflow-y-auto" {...props}>{children}</div>;
        case "List.Item":
            return <div className="supersearch-list-item px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-800 cursor-pointer" {...props}>{children}</div>;
        case "Detail":
            return <div className="supersearch-detail prose dark:prose-invert p-6" {...props}>{children}</div>;
        case "ActionPanel":
            return <div className="supersearch-action-panel absolute bottom-0 right-0 bg-white shadow-lg p-2" {...props}>{children}</div>;
        case "Action":
            return <button className="supersearch-action px-3 py-1 bg-blue-500 text-white rounded" {...props}>{props.title as string}</button>;
        case "ROOT":
            return <div className="supersearch-extension-root w-full h-full" {...props}>{children}</div>;
        default:
            console.warn(`[Hydrator] Unknown component type: ${node.type}`);
            return <div className="supersearch-unknown border border-red-500 p-2" {...props}>{children}</div>;
    }
}

export function ExtensionHydrator({ extensionId }: { extensionId: string }) {
    const [uiTree, setUiTree] = useState<UINode | null>(null);

    useEffect(() => {
        // Subscribe to UiSync events routed from this specific extension's V8 Isolate
        const unlisten = listen<{ tree: UINode }>(`extension_ui_sync_${extensionId}`, (event) => {
            setUiTree(event.tree);
        });

        return () => {
            unlisten.then(fn => fn());
        };
    }, [extensionId]);

    if (!uiTree) {
        return <div className="flex items-center justify-center h-full">Loading Extension...</div>;
    }

    return <>{renderNode(uiTree)}</>;
}
