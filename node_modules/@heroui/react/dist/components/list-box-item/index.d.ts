import type { ComponentProps } from "react";
import { ListBoxItemIndicator, ListBoxItemRoot } from "./list-box-item";
export declare const ListBoxItem: (({ children, className, variant, ...props }: import("./list-box-item").ListBoxItemRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, variant, ...props }: import("./list-box-item").ListBoxItemRootProps) => import("react/jsx-runtime").JSX.Element;
    Indicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./list-box-item").ListBoxItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./list-box-item").ListBoxItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type ListBoxItem = {
    Props: ComponentProps<typeof ListBoxItemRoot>;
    RootProps: ComponentProps<typeof ListBoxItemRoot>;
    IndicatorProps: ComponentProps<typeof ListBoxItemIndicator>;
};
export { ListBoxItemRoot, ListBoxItemIndicator };
export type { ListBoxItemRootProps, ListBoxItemRootProps as ListBoxItemProps, ListBoxItemIndicatorProps, } from "./list-box-item";
export { listboxItemVariants } from "@heroui/styles";
export type { ListBoxItemVariants } from "@heroui/styles";
