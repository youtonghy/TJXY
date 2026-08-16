import type { MenuItemRoot } from "../menu-item";
import type { MenuSectionRoot } from "../menu-section";
import type { ComponentProps } from "react";
import { MenuRoot } from "./menu";
export declare const Menu: typeof MenuRoot & {
    Root: typeof MenuRoot;
    Item: (({ children, className, variant, ...props }: import("..").MenuItemRootProps) => import("react/jsx-runtime").JSX.Element) & {
        Root: ({ children, className, variant, ...props }: import("..").MenuItemRootProps) => import("react/jsx-runtime").JSX.Element;
        Indicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, type, ...props }: import("..").MenuItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").MenuItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
        SubmenuIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("..").MenuItemSubmenuIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").MenuItemSubmenuIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element | null;
    };
    ItemIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, type, ...props }: import("..").MenuItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").MenuItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Section: ({ children, className, ...props }: import("..").MenuSectionRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type Menu<T extends object = object> = {
    Props: ComponentProps<typeof MenuRoot<T>>;
    RootProps: ComponentProps<typeof MenuRoot<T>>;
    ItemProps: ComponentProps<typeof MenuItemRoot>;
    SectionProps: ComponentProps<typeof MenuSectionRoot>;
};
export { MenuRoot };
export type { MenuRootProps, MenuRootProps as MenuProps } from "./menu";
export { menuVariants } from "@heroui/styles";
export type { MenuVariants } from "@heroui/styles";
