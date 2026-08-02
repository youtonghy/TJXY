"use client";
import { menuItemVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { MenuItem } from 'react-aria-components/Menu';
import { dom } from '../../utils/dom.js';
import { IconChevronRight } from '../icons.js';
import { jsx } from 'react/jsx-runtime';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';

const MenuItemContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Menu Item Root
 * -----------------------------------------------------------------------------------------------*/

const MenuItemRoot = ({
  children,
  className,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => menuItemVariants({
    variant
  }), [variant]);
  return /*#__PURE__*/jsx(MenuItem, {
    className: composeTwRenderProps(className, slots.item()),
    "data-slot": "menu-item",
    ...props,
    children: values => /*#__PURE__*/jsx(MenuItemContext, {
      value: {
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Menu Item Indicator
 * -----------------------------------------------------------------------------------------------*/

const MenuItemIndicator = ({
  children,
  className,
  type = "checkmark",
  ...props
}) => {
  const {
    slots,
    state
  } = use(MenuItemContext);
  const isSelected = state?.isSelected;
  const content = typeof children === "function" ? children(state ?? {}) : children ? children : type === "dot" ? /*#__PURE__*/jsx("svg", {
    "aria-hidden": "true",
    "data-slot": "menu-item-indicator--dot",
    fill: "currentColor",
    fillRule: "evenodd",
    role: "presentation",
    viewBox: "0 0 16 16",
    xmlns: "http://www.w3.org/2000/svg",
    children: /*#__PURE__*/jsx("path", {
      clipRule: "evenodd",
      d: "M8 15A7 7 0 1 0 8 1a7 7 0 0 0 0 14",
      fillRule: "evenodd"
    })
  }) : /*#__PURE__*/jsx("svg", {
    "aria-hidden": "true",
    "data-slot": "menu-item-indicator--checkmark",
    fill: "none",
    role: "presentation",
    stroke: "currentColor",
    strokeDasharray: 22,
    strokeDashoffset: isSelected ? 44 : 66,
    strokeLinecap: "round",
    strokeLinejoin: "round",
    strokeWidth: 2,
    viewBox: "0 0 17 18",
    children: /*#__PURE__*/jsx("polyline", {
      points: "1 9 7 14 15 4"
    })
  });
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.indicator, className),
    "data-slot": "menu-item-indicator",
    "data-type": type,
    "data-visible": isSelected || undefined,
    ...props,
    children: content
  });
};

/* -------------------------------------------------------------------------------------------------
 * Menu Item Submenu Indicator
 * -----------------------------------------------------------------------------------------------*/

const MenuItemSubmenuIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots,
    state
  } = use(MenuItemContext);
  const hasSubmenu = state?.hasSubmenu;
  if (!hasSubmenu) {
    return null;
  }
  const defaultContent = /*#__PURE__*/jsx(IconChevronRight, {});
  const content = children ?? defaultContent;
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.submenuIndicator, className),
    "data-slot": "submenu-indicator",
    ...props,
    children: content
  });
};

export { MenuItemIndicator, MenuItemRoot, MenuItemSubmenuIndicator };
