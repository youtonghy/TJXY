import type { ComponentProps } from "react";
import { LinkIcon, LinkRoot } from "./link";
export declare const Link: (({ children, className, ...props }: import("./link").LinkRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, ...props }: import("./link").LinkRootProps) => import("react/jsx-runtime").JSX.Element;
    Icon: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...rest }: import("./link").LinkIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./link").LinkIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type Link = {
    Props: ComponentProps<typeof LinkRoot>;
    RootProps: ComponentProps<typeof LinkRoot>;
    IconProps: ComponentProps<typeof LinkIcon>;
};
export { LinkRoot, LinkIcon };
export type { LinkRootProps, LinkIconProps, LinkRootProps as LinkProps } from "./link";
export { linkVariants } from "@heroui/styles";
export type { LinkVariants } from "@heroui/styles";
