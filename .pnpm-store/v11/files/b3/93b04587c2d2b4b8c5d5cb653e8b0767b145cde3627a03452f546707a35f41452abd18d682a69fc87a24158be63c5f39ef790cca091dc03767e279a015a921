"use client";
import { tableVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Collection, Row, ResizableTableContainer, TableLoadMoreItem as TableLoadMoreItem$1, TableHeader as TableHeader$1, Table, ColumnResizer, Column, Cell, TableBody as TableBody$1 } from 'react-aria-components/Table';
import { cx } from 'tailwind-variants';
import { composeSlotClassName, composeTwRenderProps } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronUp } from '../icons.js';
import { jsx, jsxs } from 'react/jsx-runtime';

const TableContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Table Root
 * -----------------------------------------------------------------------------------------------*/

const TableRoot = ({
  children,
  className,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => tableVariants({
    variant
  }), [variant]);
  return /*#__PURE__*/jsx(TableContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(dom.div, {
      className: slots.base({
        className
      }),
      "data-slot": "table",
      ...props,
      children: children
    })
  });
};
TableRoot.displayName = "HeroUI.Table";

/* -------------------------------------------------------------------------------------------------
 * Table Scroll Container
 * -----------------------------------------------------------------------------------------------*/

const TableScrollContainer = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.scrollContainer, className),
    "data-slot": "table-scroll-container",
    ...props
  });
};
TableScrollContainer.displayName = "HeroUI.Table.ScrollContainer";

/* -------------------------------------------------------------------------------------------------
 * Table Content
 * -----------------------------------------------------------------------------------------------*/

function TableContent({
  className,
  ...props
}) {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(Table, {
    className: composeTwRenderProps(className, slots?.content()),
    "data-slot": "table-content",
    ...props
  });
}
TableContent.displayName = "HeroUI.Table.Content";

/* -------------------------------------------------------------------------------------------------
 * Table Header
 * -----------------------------------------------------------------------------------------------*/

function TableHeader({
  className,
  ...props
}) {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(TableHeader$1, {
    className: composeTwRenderProps(className, slots?.header()),
    "data-slot": "table-header",
    ...props
  });
}
TableHeader.displayName = "HeroUI.Table.Header";

/* -------------------------------------------------------------------------------------------------
 * Table Column
 * -----------------------------------------------------------------------------------------------*/

const TableColumn = ({
  className,
  ref,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(Column, {
    ref: ref,
    className: composeTwRenderProps(className, slots?.column()),
    "data-slot": "table-column",
    ...props
  });
};
TableColumn.displayName = "HeroUI.Table.Column";

/* -------------------------------------------------------------------------------------------------
 * Table Body
 * -----------------------------------------------------------------------------------------------*/

function TableBody({
  className,
  ...props
}) {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(TableBody$1, {
    className: composeTwRenderProps(className, slots?.body()),
    "data-slot": "table-body",
    ...props
  });
}
TableBody.displayName = "HeroUI.Table.Body";

/* -------------------------------------------------------------------------------------------------
 * Table Row
 * -----------------------------------------------------------------------------------------------*/

function TableRow({
  className,
  ...props
}) {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(Row, {
    className: composeTwRenderProps(className, slots?.row()),
    "data-slot": "table-row",
    ...props
  });
}
TableRow.displayName = "HeroUI.Table.Row";

/* -------------------------------------------------------------------------------------------------
 * Table Cell
 * -----------------------------------------------------------------------------------------------*/

const TableCell = ({
  className,
  ref,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(Cell, {
    ref: ref,
    className: composeTwRenderProps(className, slots?.cell()),
    "data-slot": "table-cell",
    ...props
  });
};
TableCell.displayName = "HeroUI.Table.Cell";

/* -------------------------------------------------------------------------------------------------
 * Table Footer
 * -----------------------------------------------------------------------------------------------*/

const TableFooter = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.footer, className),
    "data-slot": "table-footer",
    ...props
  });
};
TableFooter.displayName = "HeroUI.Table.Footer";

/* -------------------------------------------------------------------------------------------------
 * Table Resizable Container
 * -----------------------------------------------------------------------------------------------*/

const TableResizableContainer = ({
  className,
  ref,
  ...props
}) => {
  return /*#__PURE__*/jsx(ResizableTableContainer, {
    ref: ref,
    className: cx("table__resizable-container", className),
    "data-slot": "table-resizable-container",
    ...props
  });
};
TableResizableContainer.displayName = "HeroUI.Table.ResizableContainer";

/* -------------------------------------------------------------------------------------------------
 * Table Column Resizer
 * -----------------------------------------------------------------------------------------------*/

const TableColumnResizer = ({
  className,
  ref,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(ColumnResizer, {
    ref: ref,
    className: composeTwRenderProps(className, slots?.columnResizer()),
    "data-slot": "table-column-resizer",
    ...props
  });
};
TableColumnResizer.displayName = "HeroUI.Table.ColumnResizer";

/* -------------------------------------------------------------------------------------------------
 * Table Load More Item
 * -----------------------------------------------------------------------------------------------*/

const TableLoadMoreItem = ({
  className,
  ref,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(TableLoadMoreItem$1, {
    ref: ref,
    className: composeSlotClassName(slots?.loadMore, className),
    "data-slot": "table-load-more",
    ...props
  });
};
TableLoadMoreItem.displayName = "HeroUI.Table.LoadMore";

/* -------------------------------------------------------------------------------------------------
 * Table Load More Content
 * -----------------------------------------------------------------------------------------------*/

const TableLoadMoreContent = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.loadMoreContent, className),
    "data-slot": "table-load-more-content",
    ...props
  });
};
TableLoadMoreContent.displayName = "HeroUI.Table.LoadMoreContent";

/* -------------------------------------------------------------------------------------------------
 * Table Sortable Column Header
 * -----------------------------------------------------------------------------------------------*/

const TableSortableColumnHeader = ({
  children,
  className,
  indicator,
  ref,
  showIndicator = true,
  sortDirection,
  ...props
}) => {
  const {
    slots
  } = use(TableContext);
  const shouldRenderIndicator = showIndicator && !!sortDirection;
  let indicatorElement = null;
  if (shouldRenderIndicator) {
    if (indicator === undefined) {
      indicatorElement = /*#__PURE__*/jsx(IconChevronUp, {
        className: slots?.sortableColumnIndicator(),
        "data-direction": sortDirection,
        "data-slot": "table-sortable-column-indicator"
      });
    } else if (/*#__PURE__*/React__default.isValidElement(indicator)) {
      const element = indicator;
      indicatorElement = /*#__PURE__*/React__default.cloneElement(element, {
        className: composeSlotClassName(slots?.sortableColumnIndicator, element.props.className),
        "data-direction": sortDirection,
        "data-slot": "table-sortable-column-indicator"
      });
    } else {
      indicatorElement = indicator;
    }
  }
  return /*#__PURE__*/jsxs("span", {
    ref: ref,
    className: composeSlotClassName(slots?.sortableColumnHeader, className),
    "data-direction": sortDirection,
    "data-slot": "table-sortable-column-header",
    ...props,
    children: [children, indicatorElement]
  });
};
TableSortableColumnHeader.displayName = "HeroUI.Table.SortableColumnHeader";

/* -------------------------------------------------------------------------------------------------
 * Exports
 * -----------------------------------------------------------------------------------------------*/
// Re-export Collection from React Aria for dynamic cell rendering within rows.
// Users wrap their dynamic cells in <Table.Collection items={columns}> when they
// need to render additional static cells (e.g. checkbox, drag handle) alongside
// dynamic column-based cells.
const TableCollection = Collection;

export { TableBody, TableCell, TableCollection, TableColumn, TableColumnResizer, TableContent, TableFooter, TableHeader, TableLoadMoreContent, TableLoadMoreItem, TableResizableContainer, TableRoot, TableRow, TableScrollContainer, TableSortableColumnHeader };
