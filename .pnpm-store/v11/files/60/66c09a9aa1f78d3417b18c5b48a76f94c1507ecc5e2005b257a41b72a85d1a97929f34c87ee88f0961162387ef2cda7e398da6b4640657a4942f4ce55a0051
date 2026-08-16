import type { KbdKey } from "./kbd.constants";
import type { DOMRenderProps } from "../../utils/dom";
import type { KbdVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
interface KbdRootProps<E extends keyof React.JSX.IntrinsicElements = "kbd"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
    /** Visual variant. */
    variant?: KbdVariants["variant"];
}
declare const KbdRoot: <E extends keyof React.JSX.IntrinsicElements = "kbd">({ children, className, variant, ...props }: KbdRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof KbdRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface KbdAbbrProps<E extends keyof React.JSX.IntrinsicElements = "abbr"> extends DOMRenderProps<E, undefined> {
    className?: string;
    /** The keyboard key to display */
    keyValue: KbdKey;
}
declare const KbdAbbr: <E extends keyof React.JSX.IntrinsicElements = "abbr">({ className, keyValue, ...props }: KbdAbbrProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof KbdAbbrProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface KbdContentProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
}
declare const KbdContent: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: KbdContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof KbdContentProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { KbdRoot, KbdAbbr, KbdContent };
export type { KbdRootProps, KbdAbbrProps, KbdContentProps };
