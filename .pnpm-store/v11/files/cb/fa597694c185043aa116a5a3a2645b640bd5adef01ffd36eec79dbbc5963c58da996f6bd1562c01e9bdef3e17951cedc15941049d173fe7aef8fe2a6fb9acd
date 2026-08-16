import type { DOMRenderProps } from "../../utils/dom";
import type { ReactNode } from "react";
import React from "react";
interface EmptyStateRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const EmptyStateRoot: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: EmptyStateRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof EmptyStateRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { EmptyStateRoot };
export type { EmptyStateRootProps };
