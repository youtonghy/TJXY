import type { DOMRenderProps } from "../../utils/dom";
import type { TableVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Cell as CellPrimitive, Collection as CollectionPrimitive, Column as ColumnPrimitive, ColumnResizer as ColumnResizerPrimitive, ResizableTableContainer as ResizableTableContainerPrimitive, Row as RowPrimitive, TableBody as TableBodyPrimitive, TableHeader as TableHeaderPrimitive, TableLoadMoreItem as TableLoadMoreItemPrimitive, Table as TablePrimitive } from "react-aria-components/Table";
interface TableRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
    /** Visual variant. */
    variant?: TableVariants["variant"];
}
declare const TableRoot: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, variant, ...props }: TableRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TableRootProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableScrollContainerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const TableScrollContainer: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: TableScrollContainerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TableScrollContainerProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableContentProps extends Omit<ComponentPropsWithRef<typeof TablePrimitive>, "className"> {
    className?: string;
}
declare function TableContent({ className, ...props }: TableContentProps): import("react/jsx-runtime").JSX.Element;
interface TableHeaderProps<T extends object> extends ComponentPropsWithRef<typeof TableHeaderPrimitive<T>> {
}
declare function TableHeader<T extends object>({ className, ...props }: TableHeaderProps<T>): import("react/jsx-runtime").JSX.Element;
interface TableColumnProps extends ComponentPropsWithRef<typeof ColumnPrimitive> {
}
declare const TableColumn: {
    ({ className, ref, ...props }: TableColumnProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableBodyProps<T extends object> extends ComponentPropsWithRef<typeof TableBodyPrimitive<T>> {
}
declare function TableBody<T extends object>({ className, ...props }: TableBodyProps<T>): import("react/jsx-runtime").JSX.Element;
interface TableRowProps<T extends object> extends ComponentPropsWithRef<typeof RowPrimitive<T>> {
}
declare function TableRow<T extends object>({ className, ...props }: TableRowProps<T>): import("react/jsx-runtime").JSX.Element;
interface TableCellProps extends ComponentPropsWithRef<typeof CellPrimitive> {
}
declare const TableCell: {
    ({ className, ref, ...props }: TableCellProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableFooterProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const TableFooter: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: TableFooterProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TableFooterProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableResizableContainerProps extends ComponentPropsWithRef<typeof ResizableTableContainerPrimitive> {
}
declare const TableResizableContainer: {
    ({ className, ref, ...props }: TableResizableContainerProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableColumnResizerProps extends ComponentPropsWithRef<typeof ColumnResizerPrimitive> {
}
declare const TableColumnResizer: {
    ({ className, ref, ...props }: TableColumnResizerProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableLoadMoreItemProps extends ComponentPropsWithRef<typeof TableLoadMoreItemPrimitive> {
}
declare const TableLoadMoreItem: {
    ({ className, ref, ...props }: TableLoadMoreItemProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface TableLoadMoreContentProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const TableLoadMoreContent: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: TableLoadMoreContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TableLoadMoreContentProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
type TableSortDirection = "ascending" | "descending";
interface TableSortableColumnHeaderProps extends Omit<React.ComponentPropsWithRef<"span">, "children"> {
    /** Label content of the column header. */
    children?: ReactNode;
    /**
     * Current sort direction for the column. Pass the `sortDirection` value
     * received from `Table.Column`'s render-prop callback.
     */
    sortDirection?: TableSortDirection;
    /**
     * Whether to render the sort indicator icon when a direction is set.
     * @default true
     */
    showIndicator?: boolean;
    /**
     * Custom indicator element. When provided, overrides the default chevron.
     * The indicator receives a `data-direction` attribute reflecting the
     * current sort direction.
     */
    indicator?: ReactNode;
}
declare const TableSortableColumnHeader: {
    ({ children, className, indicator, ref, showIndicator, sortDirection, ...props }: TableSortableColumnHeaderProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
declare const TableCollection: typeof CollectionPrimitive;
export { TableRoot, TableScrollContainer, TableContent, TableHeader, TableColumn, TableColumnResizer, TableBody, TableRow, TableCell, TableFooter, TableCollection, TableLoadMoreItem, TableLoadMoreContent, TableResizableContainer, TableSortableColumnHeader, };
export type { TableRootProps, TableScrollContainerProps, TableContentProps, TableHeaderProps, TableColumnProps, TableColumnResizerProps, TableBodyProps, TableRowProps, TableCellProps, TableFooterProps, TableLoadMoreItemProps, TableLoadMoreContentProps, TableResizableContainerProps, TableSortableColumnHeaderProps, TableSortDirection, };
