/**
 * Custom React Reconciler (Phase 5 / Milestone 3)
 *
 * Translates React JSX into an abstract `UINode` tree, stripping non-serializable
 * closures and replacing them with `CallbackID`s. On `resetAfterCommit`, it batches
 * the updated tree onto a microtask and pushes it to the Rust Host via `op_ipc_post`.
 */

import type { ReactNode } from "react";
import ReactReconciler from "react-reconciler";
import { DefaultEventPriority } from "react-reconciler/constants";
import { extractEvents, garbageCollectEvents } from "./registry";
import { postUiSync } from "./ipc";

export interface UINode {
    id: string;
    type: string;
    props: Record<string, unknown>;
    events: Record<string, string>;
    children: UINode[];
    text?: string;
}

let nodeCounter = 0;

/**
 * The `react-reconciler` host config. Every mutation method below operates on the
 * plain `UINode` object graph — never the DOM — which is what makes this renderer
 * "native": the tree it builds is exactly the payload the Rust host receives.
 */
const hostConfig: ReactReconciler.HostConfig<
    string, // Type
    Record<string, unknown>, // Props
    UINode, // Container
    UINode, // Instance
    UINode, // TextInstance
    UINode, // SuspenseInstance
    UINode, // HydratableInstance
    UINode, // PublicInstance
    Record<string, never>, // HostContext
    Record<string, unknown>, // UpdatePayload
    unknown[], // ChildSet
    number, // TimeoutHandle
    -1 // NoTimeout
> = {
    supportsMutation: true,
    supportsPersistence: false,
    supportsHydration: false,
    isPrimaryRenderer: true,
    noTimeout: -1,
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,

    getRootHostContext: () => ({}),
    getChildHostContext: (parentContext) => parentContext,
    getPublicInstance: (instance) => instance,
    prepareForCommit: () => null,
    resetAfterCommit: (containerInfo) => {
        // Batch: `resetAfterCommit` fires once per completed Fiber commit. Deferring the
        // actual post to a microtask lets multiple synchronous `setState` calls collapse
        // into a single IPC round trip (Phase 5 §4 — Serialization Pipeline & Batching).
        queueMicrotask(() => postUiSync(containerInfo));
    },

    createInstance: (type, props): UINode => {
        const [safeProps, events] = extractEvents(props);
        return { id: `n_${++nodeCounter}`, type, props: safeProps, events, children: [] };
    },

    createTextInstance: (text): UINode => ({
        id: `t_${++nodeCounter}`,
        type: "TEXT",
        props: {},
        events: {},
        children: [],
        text,
    }),

    appendInitialChild: (parent, child) => {
        parent.children.push(child);
    },
    appendChild: (parent, child) => {
        parent.children.push(child);
    },
    appendChildToContainer: (container, child) => {
        container.children.push(child);
    },

    insertBefore: (parent, child, beforeChild) => {
        const index = parent.children.indexOf(beforeChild);
        parent.children.splice(index === -1 ? parent.children.length : index, 0, child);
    },
    insertInContainerBefore: (container, child, beforeChild) => {
        const index = container.children.indexOf(beforeChild);
        container.children.splice(index === -1 ? container.children.length : index, 0, child);
    },

    removeChild: (parent, child) => {
        garbageCollectEvents(child.events);
        const index = parent.children.indexOf(child);
        if (index !== -1) parent.children.splice(index, 1);
    },
    removeChildFromContainer: (container, child) => {
        garbageCollectEvents(child.events);
        const index = container.children.indexOf(child);
        if (index !== -1) container.children.splice(index, 1);
    },

    prepareUpdate: (_instance, _type, _oldProps, newProps) => newProps,
    commitUpdate: (instance, _updatePayload, _type, _oldProps, newProps) => {
        garbageCollectEvents(instance.events);
        const [safeProps, events] = extractEvents(newProps);
        instance.props = safeProps;
        instance.events = events;
    },
    commitTextUpdate: (textInstance, _oldText, newText) => {
        textInstance.text = newText;
    },
    clearContainer: (container) => {
        for (const child of container.children) garbageCollectEvents(child.events);
        container.children = [];
    },

    shouldSetTextContent: () => false,
    finalizeInitialChildren: () => false,
    detachDeletedInstance: () => {},
    preparePortalMount: () => {},
    getCurrentEventPriority: () => DefaultEventPriority,
    getInstanceFromNode: () => null,
    getInstanceFromScope: () => null,
    beforeActiveInstanceBlur: () => {},
    afterActiveInstanceBlur: () => {},
    prepareScopeUpdate: () => {},
};

const reconciler = ReactReconciler(hostConfig);

/**
 * Mounts `element` as the extension's active view. Called once per command invocation;
 * subsequent state updates flow through the reconciler's normal Fiber scheduling.
 */
export function render(element: ReactNode): void {
    const rootNode: UINode = { id: "root", type: "ROOT", props: {}, events: {}, children: [] };
    const container = reconciler.createContainer(
        rootNode,
        0, // LegacyRoot
        null,
        false,
        null,
        "",
        console.error,
        null,
    );
    reconciler.updateContainer(element, container, null, null);
}

export { startEventLoop, awaitResponse } from "./eventLoop";
export { postRequest, postUiSync } from "./ipc";
