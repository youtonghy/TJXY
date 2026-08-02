import type { DOMRenderProps } from "../../utils/dom";
import type { ReactNode } from "react";
import React from "react";
interface FieldsetRootProps<E extends keyof React.JSX.IntrinsicElements = "fieldset"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const FieldsetRoot: <E extends keyof React.JSX.IntrinsicElements = "fieldset">({ children, className, ...props }: FieldsetRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof FieldsetRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface FieldsetLegendProps<E extends keyof React.JSX.IntrinsicElements = "legend"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const FieldsetLegend: <E extends keyof React.JSX.IntrinsicElements = "legend">({ className, ...props }: FieldsetLegendProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof FieldsetLegendProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface FieldGroupProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const FieldGroup: <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...rest }: FieldGroupProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof FieldGroupProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface FieldsetActionsProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const FieldsetActions: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: FieldsetActionsProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof FieldsetActionsProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { FieldsetRoot, FieldsetLegend, FieldGroup, FieldsetActions };
export type { FieldsetRootProps, FieldsetLegendProps, FieldGroupProps, FieldsetActionsProps };
