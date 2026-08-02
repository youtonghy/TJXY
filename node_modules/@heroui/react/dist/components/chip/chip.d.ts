import type { DOMRenderProps } from "../../utils/dom";
import type { ChipVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
interface ChipRootProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
    /** Chip color. */
    color?: ChipVariants["color"];
    /** Chip size. */
    size?: ChipVariants["size"];
    /** Chip variant. */
    variant?: ChipVariants["variant"];
}
declare const ChipRoot: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, color, size, variant, ...props }: ChipRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ChipRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ChipLabelProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ChipLabel: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: ChipLabelProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ChipLabelProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { ChipRoot, ChipLabel };
export type { ChipRootProps, ChipLabelProps };
