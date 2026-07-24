/**
 * Declarative UI primitives (Phase 6 & 7).
 *
 * These components are strictly typed shells. They render into the Reconciler's
 * `UINode` tree and are serialized to the host application, which provides the
 * actual native layout and styling.
 */

import React, { createContext, useContext, useState, ReactNode } from "react";

export interface ListProps {
    isLoading?: boolean;
    searchText?: string;
    onSearchTextChange?: (text: string) => void;
    onSelectionChange?: (id: string) => void;
    navigationTitle?: string;
    searchBarPlaceholder?: string;
    children: ReactNode;
}

export function List(props: ListProps) {
    return React.createElement("List", props);
}

export interface ListItemProps {
    id?: string;
    title: string;
    subtitle?: string;
    icon?: string;
    actions?: ReactNode;
}

List.Item = function ListItem(props: ListItemProps) {
    return React.createElement("List.Item", props);
};

export interface DetailProps {
    markdown: string;
    actions?: ReactNode;
}

export function Detail(props: DetailProps) {
    return React.createElement("Detail", props);
}

export interface ActionPanelProps {
    title?: string;
    children: ReactNode;
}

export function ActionPanel(props: ActionPanelProps) {
    return React.createElement("ActionPanel", props);
}

export interface ActionProps {
    title: string;
    onAction?: () => void;
    icon?: string;
}

export function Action(props: ActionProps) {
    return React.createElement("Action", props);
}

/** 
 * Navigation context to manage the view stack.
 *
 * Implemented locally within the guest, mapping directly to conditionally rendered
 * components at the root level, meaning the host always receives the active view's tree.
 */
interface NavigationState {
    push: (view: ReactNode) => void;
    pop: () => void;
}

const NavigationContext = createContext<NavigationState | null>(null);

export function useNavigation(): NavigationState {
    const context = useContext(NavigationContext);
    if (!context) {
        throw new Error("useNavigation must be used within a SuperSearch extension component.");
    }
    return context;
}

/** 
 * Root container injected by the platform bootstrap to handle navigation stacks. 
 */
export function ExtensionRoot({ children }: { children: ReactNode }) {
    const [stack, setStack] = useState<ReactNode[]>([children]);

    const push = (view: ReactNode) => setStack((prev) => [...prev, view]);
    const pop = () => setStack((prev) => (prev.length > 1 ? prev.slice(0, -1) : prev));

    const activeView = stack[stack.length - 1];

    return React.createElement(
        NavigationContext.Provider,
        { value: { push, pop } },
        activeView
    );
}
