"use client";
import { paginationVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { composeSlotClassName, composeTwRenderProps } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronLeft, IconChevronRight } from '../icons.js';
import { jsx } from 'react/jsx-runtime';

const PaginationContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Pagination Root
 * -----------------------------------------------------------------------------------------------*/

const PaginationRoot = ({
  children,
  className,
  size,
  ...props
}) => {
  const slots = React__default.useMemo(() => paginationVariants({
    size
  }), [size]);
  return /*#__PURE__*/jsx(PaginationContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(dom.nav, {
      "aria-label": "pagination",
      "data-slot": "pagination",
      role: "navigation",
      ...props,
      className: composeSlotClassName(slots.base, className),
      children: children
    })
  });
};
PaginationRoot.displayName = "HeroUI.Pagination";

/* -------------------------------------------------------------------------------------------------
 * Pagination Summary
 * -----------------------------------------------------------------------------------------------*/

const PaginationSummary = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.summary, className),
    "data-slot": "pagination-summary"
    // Auto-detect base direction from the summary's own text so mixed
    // number/word content (e.g. "1 to 5 of 10 invoices") is not bidi-reordered
    // by an RTL ancestor. User-provided `dir` still overrides via `...props`.
    ,
    dir: "auto",
    ...props,
    children: children
  });
};
PaginationSummary.displayName = "HeroUI.Pagination.Summary";

/* -------------------------------------------------------------------------------------------------
 * Pagination Content
 * -----------------------------------------------------------------------------------------------*/

const PaginationContent = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  return /*#__PURE__*/jsx(dom.ul, {
    className: composeSlotClassName(slots?.content, className),
    "data-slot": "pagination-content",
    ...props,
    children: children
  });
};
PaginationContent.displayName = "HeroUI.Pagination.Content";

/* -------------------------------------------------------------------------------------------------
 * Pagination Item
 * -----------------------------------------------------------------------------------------------*/

const PaginationItem = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  return /*#__PURE__*/jsx(dom.li, {
    className: composeSlotClassName(slots?.item, className),
    "data-slot": "pagination-item",
    ...props,
    children: children
  });
};
PaginationItem.displayName = "HeroUI.Pagination.Item";

/* -------------------------------------------------------------------------------------------------
 * Pagination Link
 * -----------------------------------------------------------------------------------------------*/

const PaginationLink = ({
  children,
  className,
  isActive,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  return /*#__PURE__*/jsx(Button, {
    "aria-current": isActive ? "page" : undefined,
    className: composeTwRenderProps(className, slots?.link()),
    "data-active": isActive ? "true" : undefined,
    "data-slot": "pagination-link",
    ...props,
    children: children
  });
};
PaginationLink.displayName = "HeroUI.Pagination.Link";

/* -------------------------------------------------------------------------------------------------
 * Pagination Previous
 * -----------------------------------------------------------------------------------------------*/

const PaginationPrevious = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  const baseClass = `${slots?.link() ?? ""} pagination__link--nav`.trim();
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, baseClass),
    "data-slot": "pagination-previous",
    ...props,
    children: children
  });
};
PaginationPrevious.displayName = "HeroUI.Pagination.Previous";

/* -------------------------------------------------------------------------------------------------
 * Pagination Previous Icon
 * -----------------------------------------------------------------------------------------------*/

const PaginationPreviousIcon = ({
  children,
  className,
  ...props
}) => {
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: className,
    "data-slot": "pagination-previous-icon",
    ...props,
    children: children ?? /*#__PURE__*/jsx(IconChevronLeft, {})
  });
};
PaginationPreviousIcon.displayName = "HeroUI.Pagination.PreviousIcon";

/* -------------------------------------------------------------------------------------------------
 * Pagination Next
 * -----------------------------------------------------------------------------------------------*/

const PaginationNext = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  const baseClass = `${slots?.link() ?? ""} pagination__link--nav`.trim();
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, baseClass),
    "data-slot": "pagination-next",
    ...props,
    children: children
  });
};
PaginationNext.displayName = "HeroUI.Pagination.Next";

/* -------------------------------------------------------------------------------------------------
 * Pagination Next Icon
 * -----------------------------------------------------------------------------------------------*/

const PaginationNextIcon = ({
  children,
  className,
  ...props
}) => {
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: className,
    "data-slot": "pagination-next-icon",
    ...props,
    children: children ?? /*#__PURE__*/jsx(IconChevronRight, {})
  });
};
PaginationNextIcon.displayName = "HeroUI.Pagination.NextIcon";

/* -------------------------------------------------------------------------------------------------
 * Pagination Ellipsis
 * -----------------------------------------------------------------------------------------------*/

const PaginationEllipsis = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(PaginationContext);
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.ellipsis, className),
    "data-slot": "pagination-ellipsis",
    ...props,
    children: "\u2026"
  });
};
PaginationEllipsis.displayName = "HeroUI.Pagination.Ellipsis";

export { PaginationContent, PaginationEllipsis, PaginationItem, PaginationLink, PaginationNext, PaginationNextIcon, PaginationPrevious, PaginationPreviousIcon, PaginationRoot, PaginationSummary };
