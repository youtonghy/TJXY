import type { ComponentProps } from "react";
import { PaginationContent, PaginationEllipsis, PaginationItem, PaginationLink, PaginationNext, PaginationNextIcon, PaginationPrevious, PaginationPreviousIcon, PaginationRoot, PaginationSummary } from "./pagination";
export declare const Pagination: {
    <E extends keyof React.JSX.IntrinsicElements = "nav">({ children, className, size, ...props }: import("./pagination").PaginationRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationRootProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Content: {
        <E extends keyof React.JSX.IntrinsicElements = "ul">({ children, className, ...props }: import("./pagination").PaginationContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationContentProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Ellipsis: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: import("./pagination").PaginationEllipsisProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationEllipsisProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Item: {
        <E extends keyof React.JSX.IntrinsicElements = "li">({ children, className, ...props }: import("./pagination").PaginationItemProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationItemProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Link: {
        ({ children, className, isActive, ...props }: import("./pagination").PaginationLinkProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Next: {
        ({ children, className, ...props }: import("./pagination").PaginationNextProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    NextIcon: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./pagination").PaginationNextIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationNextIconProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Previous: {
        ({ children, className, ...props }: import("./pagination").PaginationPreviousProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    PreviousIcon: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./pagination").PaginationPreviousIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationPreviousIconProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Root: {
        <E extends keyof React.JSX.IntrinsicElements = "nav">({ children, className, size, ...props }: import("./pagination").PaginationRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationRootProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Summary: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./pagination").PaginationSummaryProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./pagination").PaginationSummaryProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type Pagination = {
    ContentProps: ComponentProps<typeof PaginationContent>;
    EllipsisProps: ComponentProps<typeof PaginationEllipsis>;
    ItemProps: ComponentProps<typeof PaginationItem>;
    LinkProps: ComponentProps<typeof PaginationLink>;
    NextIconProps: ComponentProps<typeof PaginationNextIcon>;
    NextProps: ComponentProps<typeof PaginationNext>;
    PreviousIconProps: ComponentProps<typeof PaginationPreviousIcon>;
    PreviousProps: ComponentProps<typeof PaginationPrevious>;
    Props: ComponentProps<typeof PaginationRoot>;
    RootProps: ComponentProps<typeof PaginationRoot>;
    SummaryProps: ComponentProps<typeof PaginationSummary>;
};
export { PaginationRoot, PaginationSummary, PaginationContent, PaginationItem, PaginationLink, PaginationPrevious, PaginationPreviousIcon, PaginationNext, PaginationNextIcon, PaginationEllipsis, };
export type { PaginationRootProps, PaginationRootProps as PaginationProps, PaginationSummaryProps, PaginationContentProps, PaginationItemProps, PaginationLinkProps, PaginationPreviousProps, PaginationPreviousIconProps, PaginationNextProps, PaginationNextIconProps, PaginationEllipsisProps, } from "./pagination";
export { paginationVariants } from "@heroui/styles";
export type { PaginationVariants } from "@heroui/styles";
