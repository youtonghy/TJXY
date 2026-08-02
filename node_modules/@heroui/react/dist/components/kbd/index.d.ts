import type { ComponentProps } from "react";
import { KbdAbbr, KbdContent, KbdRoot } from "./kbd";
export declare const Kbd: (<E extends keyof React.JSX.IntrinsicElements = "kbd">({ children, className, variant, ...props }: import("./kbd").KbdRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./kbd").KbdRootProps<E>>) => import("react/jsx-runtime").JSX.Element) & {
    Root: <E extends keyof React.JSX.IntrinsicElements = "kbd">({ children, className, variant, ...props }: import("./kbd").KbdRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./kbd").KbdRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Abbr: <E extends keyof React.JSX.IntrinsicElements = "abbr">({ className, keyValue, ...props }: import("./kbd").KbdAbbrProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./kbd").KbdAbbrProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Content: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./kbd").KbdContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./kbd").KbdContentProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type Kbd = {
    Props: ComponentProps<typeof KbdRoot>;
    RootProps: ComponentProps<typeof KbdRoot>;
    AbbrProps: ComponentProps<typeof KbdAbbr>;
    ContentProps: ComponentProps<typeof KbdContent>;
};
export { KbdRoot, KbdAbbr, KbdContent };
export type { KbdRootProps, KbdAbbrProps, KbdContentProps, KbdRootProps as KbdProps } from "./kbd";
export { kbdVariants } from "@heroui/styles";
export type { KbdVariants } from "@heroui/styles";
export { kbdKeysMap, kbdKeysLabelMap } from "./kbd.constants";
export type { KbdKey } from "./kbd.constants";
