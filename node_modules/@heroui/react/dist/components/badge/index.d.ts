import type { ComponentProps } from "react";
import { BadgeAnchor, BadgeLabel, BadgeRoot } from "./badge";
export declare const Badge: (<E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, color, placement, size, variant, ...props }: import("./badge").BadgeRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./badge").BadgeRootProps<E>>) => import("react/jsx-runtime").JSX.Element) & {
    Anchor: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./badge").BadgeAnchorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./badge").BadgeAnchorProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Label: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./badge").BadgeLabelProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./badge").BadgeLabelProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Root: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, color, placement, size, variant, ...props }: import("./badge").BadgeRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./badge").BadgeRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type Badge = {
    AnchorProps: ComponentProps<typeof BadgeAnchor>;
    LabelProps: ComponentProps<typeof BadgeLabel>;
    Props: ComponentProps<typeof BadgeRoot>;
    RootProps: ComponentProps<typeof BadgeRoot>;
};
export { BadgeRoot, BadgeLabel, BadgeAnchor };
export type { BadgeRootProps, BadgeRootProps as BadgeProps, BadgeLabelProps, BadgeAnchorProps, } from "./badge";
export { badgeVariants } from "@heroui/styles";
export type { BadgeVariants } from "@heroui/styles";
