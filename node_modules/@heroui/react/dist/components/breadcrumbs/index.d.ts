import type { ComponentProps } from "react";
import { BreadcrumbsItem, BreadcrumbsRoot } from "./breadcrumbs";
export declare const Breadcrumbs: (({ children, className, separator, ...props }: import("./breadcrumbs").BreadcrumbsRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, separator, ...props }: import("./breadcrumbs").BreadcrumbsRootProps) => import("react/jsx-runtime").JSX.Element;
    Item: ({ children, className, ...props }: import("./breadcrumbs").BreadcrumbsItemProps & Omit<import("..").LinkRootProps, "className">) => import("react/jsx-runtime").JSX.Element;
};
export type Breadcrumbs = {
    Props: ComponentProps<typeof BreadcrumbsRoot>;
    RootProps: ComponentProps<typeof BreadcrumbsRoot>;
    ItemProps: ComponentProps<typeof BreadcrumbsItem>;
};
export { BreadcrumbsRoot, BreadcrumbsItem };
export type { BreadcrumbsRootProps, BreadcrumbsRootProps as BreadcrumbsProps, BreadcrumbsItemProps, } from "./breadcrumbs";
export { breadcrumbsVariants } from "@heroui/styles";
export type { BreadcrumbsVariants } from "@heroui/styles";
