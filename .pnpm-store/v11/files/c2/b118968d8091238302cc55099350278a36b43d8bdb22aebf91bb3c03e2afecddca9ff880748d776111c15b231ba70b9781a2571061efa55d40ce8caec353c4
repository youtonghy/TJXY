import type { DOMRenderProps } from "../../utils/dom";
import type { TabsVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { SelectionIndicator as SelectionIndicatorPrimitive } from "react-aria-components/SelectionIndicator";
import { TabList as TabListPrimitive, TabPanel as TabPanelPrimitive, Tab as TabPrimitive, Tabs as TabsPrimitive } from "react-aria-components/Tabs";
interface TabsRootProps extends ComponentPropsWithRef<typeof TabsPrimitive>, TabsVariants {
    children: React.ReactNode;
    className?: string;
}
declare const TabsRoot: ({ children, className, orientation, variant, ...props }: TabsRootProps) => import("react/jsx-runtime").JSX.Element;
interface TabListContainerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const TabListContainer: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, render, ...containerProps }: TabListContainerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TabListContainerProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface TabListProps extends ComponentPropsWithRef<typeof TabListPrimitive<object>> {
    children: React.ReactNode;
    className?: string;
}
declare const TabList: ({ children, className, ...props }: TabListProps) => import("react/jsx-runtime").JSX.Element;
interface TabProps extends ComponentPropsWithRef<typeof TabPrimitive> {
    className?: string;
}
declare const Tab: ({ children, className, ...props }: TabProps) => import("react/jsx-runtime").JSX.Element;
interface TabIndicatorProps extends ComponentPropsWithRef<typeof SelectionIndicatorPrimitive> {
    className?: string;
}
declare const TabIndicator: ({ className, ...props }: TabIndicatorProps) => import("react/jsx-runtime").JSX.Element;
interface TabPanelProps extends Omit<ComponentPropsWithRef<typeof TabPanelPrimitive>, "children"> {
    children: React.ReactNode;
    className?: string;
}
declare const TabPanel: ({ children, className, ...props }: TabPanelProps) => import("react/jsx-runtime").JSX.Element;
interface TabSeparatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    className?: string;
}
declare const TabSeparator: <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: TabSeparatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TabSeparatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { TabsRoot, TabListContainer, TabList, Tab, TabIndicator, TabPanel, TabSeparator };
export type { TabsRootProps, TabListContainerProps, TabListProps, TabProps, TabIndicatorProps, TabPanelProps, TabSeparatorProps, };
