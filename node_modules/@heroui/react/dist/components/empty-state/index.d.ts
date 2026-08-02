import type { ComponentProps } from "react";
import { EmptyStateRoot } from "./empty-state";
export declare const EmptyState: (<E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: import("./empty-state").EmptyStateRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./empty-state").EmptyStateRootProps<E>>) => import("react/jsx-runtime").JSX.Element) & {
    Root: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: import("./empty-state").EmptyStateRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./empty-state").EmptyStateRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type EmptyState = {
    Props: ComponentProps<typeof EmptyStateRoot>;
    RootProps: ComponentProps<typeof EmptyStateRoot>;
};
export { EmptyStateRoot };
export type { EmptyStateRootProps, EmptyStateRootProps as EmptyStateProps } from "./empty-state";
export { emptyStateVariants } from "@heroui/styles";
export type { EmptyStateVariants } from "@heroui/styles";
