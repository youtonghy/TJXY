import type { DOMRenderProps } from "../../utils/dom";
import type { SpinnerVariants } from "@heroui/styles";
import React from "react";
interface SpinnerRootProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    className?: string;
    /** Spinner color. */
    color?: SpinnerVariants["color"];
    /** Spinner size. */
    size?: SpinnerVariants["size"];
}
declare const SpinnerRoot: <E extends keyof React.JSX.IntrinsicElements = "span">({ className, color, size, ...props }: SpinnerRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SpinnerRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { SpinnerRoot };
export type { SpinnerRootProps };
