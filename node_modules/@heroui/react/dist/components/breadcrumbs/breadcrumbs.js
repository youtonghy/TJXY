"use client";
import { breadcrumbsVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Breadcrumbs, Breadcrumb } from 'react-aria-components/Breadcrumbs';
import { composeSlotClassName, composeTwRenderProps } from '../../utils/compose.js';
import { IconChevronRight } from '../icons.js';
import { Link } from '../link/index.js';
import { jsx, jsxs, Fragment } from 'react/jsx-runtime';

const BreadcrumbsContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Breadcrumbs Root
 * -----------------------------------------------------------------------------------------------*/

const BreadcrumbsRoot = ({
  children,
  className,
  separator,
  ...props
}) => {
  const slots = React__default.useMemo(() => breadcrumbsVariants({}), []);
  return /*#__PURE__*/jsx(BreadcrumbsContext.Provider, {
    value: {
      slots,
      separator
    },
    children: /*#__PURE__*/jsx(Breadcrumbs, {
      "data-slot": "breadcrumbs",
      ...props,
      className: composeSlotClassName(slots.base, className),
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Breadcrumbs Item
 * -----------------------------------------------------------------------------------------------*/

const BreadcrumbsItem = ({
  children,
  className,
  ...props
}) => {
  const {
    separator,
    slots
  } = use(BreadcrumbsContext);
  const renderSeparator = () => {
    if (!separator) return /*#__PURE__*/jsx(IconChevronRight, {
      className: slots?.separator(),
      "data-slot": "breadcrumbs-separator"
    });
    if (/*#__PURE__*/React__default.isValidElement(separator)) {
      return /*#__PURE__*/React__default.cloneElement(separator, {
        className: slots?.separator(),
        "data-slot": "breadcrumbs-separator"
      });
    }
    return separator;
  };
  return /*#__PURE__*/jsx(Breadcrumb, {
    className: composeTwRenderProps(className, slots?.item()),
    "data-slot": "breadcrumbs-item",
    ...props,
    children: ({
      isCurrent
    }) => {
      if (typeof children === "function") {
        return children({});
      }
      return /*#__PURE__*/jsxs(Fragment, {
        children: [/*#__PURE__*/jsx(Link, {
          className: slots?.link(),
          ...props,
          children: children
        }), !isCurrent && renderSeparator()]
      });
    }
  });
};

export { BreadcrumbsItem, BreadcrumbsRoot };
