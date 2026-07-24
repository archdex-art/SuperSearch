/**
 * Gate B fixture — the smallest possible extension.
 *
 * Deliberately bypasses React/the reconciler's Fiber lifecycle: this exists only to
 * prove the runtime contract (bundle → V8 → postUiSync → MessagePack → Rust decode),
 * not to exercise the reconciler itself. That is a separate, already-tested surface.
 */
import { postUiSync } from "@supersearch/reconciler";

postUiSync({
    id: "root",
    type: "ROOT",
    props: {},
    events: {},
    children: [
        {
            id: "n_1",
            type: "List.Item",
            props: { title: "Hello from the sandbox" },
            events: {},
            children: [],
        },
    ],
});
