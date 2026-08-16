import type { DOMRenderProps } from "../../utils/dom";
import type { AccordionVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Button } from "react-aria-components/Button";
import { Disclosure, Heading as DisclosureHeading, DisclosurePanel } from "react-aria-components/Disclosure";
import { DisclosureGroup } from "react-aria-components/DisclosureGroup";
interface AccordionRootProps extends ComponentPropsWithRef<typeof DisclosureGroup>, AccordionVariants {
    hideSeparator?: boolean;
}
declare const AccordionRoot: ({ children, className, hideSeparator, variant, ...props }: AccordionRootProps) => import("react/jsx-runtime").JSX.Element;
interface AccordionItemProps extends ComponentPropsWithRef<typeof Disclosure> {
}
declare const AccordionItem: ({ className, ...props }: AccordionItemProps) => import("react/jsx-runtime").JSX.Element;
interface AccordionIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "svg"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AccordionIndicator: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: AccordionIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AccordionIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AccordionHeadingProps extends ComponentPropsWithRef<typeof DisclosureHeading> {
    className?: string;
}
declare const AccordionHeading: ({ className, ...props }: AccordionHeadingProps) => import("react/jsx-runtime").JSX.Element;
interface AccordionTriggerProps extends ComponentPropsWithRef<typeof Button> {
}
declare const AccordionTrigger: ({ className, ...props }: AccordionTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface AccordionBodyProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AccordionBody: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: AccordionBodyProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AccordionBodyProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AccordionPanelProps extends ComponentPropsWithRef<typeof DisclosurePanel> {
}
declare const AccordionPanel: ({ children, className, ...props }: AccordionPanelProps) => import("react/jsx-runtime").JSX.Element;
export { AccordionRoot, AccordionItem, AccordionTrigger, AccordionPanel, AccordionIndicator, AccordionBody, AccordionHeading, };
export type { AccordionRootProps, AccordionItemProps, AccordionTriggerProps, AccordionPanelProps, AccordionIndicatorProps, AccordionBodyProps, AccordionHeadingProps, };
