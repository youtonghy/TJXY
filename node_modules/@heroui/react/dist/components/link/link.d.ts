import type { DOMRenderProps } from "../../utils/dom";
import type { LinkVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Link as LinkPrimitive } from "react-aria-components/Link";
interface LinkRootProps extends ComponentPropsWithRef<typeof LinkPrimitive>, LinkVariants {
}
declare const LinkRoot: ({ children, className, ...props }: LinkRootProps) => import("react/jsx-runtime").JSX.Element;
interface LinkIconProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const LinkIcon: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...rest }: LinkIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof LinkIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { LinkRoot, LinkIcon };
export type { LinkRootProps, LinkIconProps };
