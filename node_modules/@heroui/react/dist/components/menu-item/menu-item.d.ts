import type { DOMRenderProps } from "../../utils/dom";
import type { MenuItemVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { MenuItemRenderProps } from "react-aria-components/Menu";
import React from "react";
import { MenuItem as MenuItemPrimitive } from "react-aria-components/Menu";
interface MenuItemRootProps extends ComponentPropsWithRef<typeof MenuItemPrimitive>, MenuItemVariants {
    className?: string;
}
declare const MenuItemRoot: ({ children, className, variant, ...props }: MenuItemRootProps) => import("react/jsx-runtime").JSX.Element;
interface MenuItemIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode | ((props: MenuItemRenderProps) => React.ReactNode);
    className?: string;
    type?: "checkmark" | "dot";
}
declare const MenuItemIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, type, ...props }: MenuItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof MenuItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface MenuItemSubmenuIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode;
    className?: string;
}
declare const MenuItemSubmenuIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: MenuItemSubmenuIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof MenuItemSubmenuIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element | null;
export { MenuItemRoot, MenuItemIndicator, MenuItemSubmenuIndicator };
export type { MenuItemRootProps, MenuItemIndicatorProps, MenuItemSubmenuIndicatorProps };
