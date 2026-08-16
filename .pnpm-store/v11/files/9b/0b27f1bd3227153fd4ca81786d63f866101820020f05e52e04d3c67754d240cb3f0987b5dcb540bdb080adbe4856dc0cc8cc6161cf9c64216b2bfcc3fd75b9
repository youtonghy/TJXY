import type { LinkProps } from "../link";
import type { BreadcrumbsVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { Breadcrumb as BreadcrumbPrimitive, Breadcrumbs as BreadcrumbsPrimitive } from "react-aria-components/Breadcrumbs";
interface BreadcrumbsRootProps extends ComponentPropsWithRef<typeof BreadcrumbsPrimitive>, BreadcrumbsVariants {
    separator?: React.ReactNode;
}
declare const BreadcrumbsRoot: ({ children, className, separator, ...props }: BreadcrumbsRootProps) => import("react/jsx-runtime").JSX.Element;
interface BreadcrumbsItemProps extends ComponentPropsWithRef<typeof BreadcrumbPrimitive> {
}
declare const BreadcrumbsItem: ({ children, className, ...props }: BreadcrumbsItemProps & Omit<LinkProps, "className">) => import("react/jsx-runtime").JSX.Element;
export { BreadcrumbsRoot, BreadcrumbsItem };
export type { BreadcrumbsRootProps, BreadcrumbsItemProps };
