import type { DOMRenderProps } from "../../utils/dom";
import type { ListBoxItemVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { ListBoxItemRenderProps } from "react-aria-components/ListBox";
import React from "react";
import { ListBoxItem as ListBoxItemPrimitive } from "react-aria-components/ListBox";
interface ListBoxItemRootProps extends ComponentPropsWithRef<typeof ListBoxItemPrimitive>, ListBoxItemVariants {
    className?: string;
}
declare const ListBoxItemRoot: ({ children, className, variant, ...props }: ListBoxItemRootProps) => import("react/jsx-runtime").JSX.Element;
interface ListBoxItemIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode | ((props: ListBoxItemRenderProps) => React.ReactNode);
    className?: string;
}
declare const ListBoxItemIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: ListBoxItemIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ListBoxItemIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { ListBoxItemRoot, ListBoxItemIndicator };
export type { ListBoxItemRootProps, ListBoxItemIndicatorProps };
