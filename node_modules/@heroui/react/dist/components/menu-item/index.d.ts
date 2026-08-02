import type { ComponentProps } from "react";
import { MenuItemIndicator, MenuItemRoot, MenuItemSubmenuIndicator } from "./menu-item";
export declare const MenuItem: (({ children, className, variant, ...props }: import("./menu-item").MenuItemRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, variant, ...props }: import("./menu-item").MenuItemRootProps) => import("react/jsx-runtime").JSX.Element;
    Indicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, type, ...props }: import("./menu-item").MenuItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./menu-item").MenuItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
    SubmenuIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./menu-item").MenuItemSubmenuIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./menu-item").MenuItemSubmenuIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element | null;
};
export type MenuItem = {
    Props: ComponentProps<typeof MenuItemRoot>;
    RootProps: ComponentProps<typeof MenuItemRoot>;
    IndicatorProps: ComponentProps<typeof MenuItemIndicator>;
    SubmenuIndicatorProps: ComponentProps<typeof MenuItemSubmenuIndicator>;
};
export { MenuItemRoot, MenuItemIndicator, MenuItemSubmenuIndicator };
export type { MenuItemRootProps, MenuItemRootProps as MenuItemProps, MenuItemIndicatorProps, MenuItemSubmenuIndicatorProps, } from "./menu-item";
export { menuItemVariants } from "@heroui/styles";
export type { MenuItemVariants } from "@heroui/styles";
