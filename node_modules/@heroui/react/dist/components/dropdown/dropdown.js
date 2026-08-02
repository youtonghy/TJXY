"use client";
import { dropdownVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { MenuTrigger, SubmenuTrigger, Menu, Popover } from 'react-aria-components/Menu';
import { composeTwRenderProps } from '../../utils/compose.js';
import '../menu-item/index.js';
import { MenuSectionRoot } from '../menu-section/menu-section.js';
import { jsx, Fragment } from 'react/jsx-runtime';
import { MenuItemSubmenuIndicator, MenuItemIndicator, MenuItemRoot } from '../menu-item/menu-item.js';
import { SurfaceContext } from '../surface/surface.js';

const DropdownContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Dropdown Root (MenuTrigger wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownRoot = ({
  children,
  ...props
}) => {
  const slots = React__default.useMemo(() => dropdownVariants(), []);
  return /*#__PURE__*/jsx(DropdownContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(MenuTrigger, {
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Trigger (Button wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DropdownContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "dropdown-trigger",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Popover (Popover wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownPopover = ({
  children,
  className,
  placement,
  ...props
}) => {
  const {
    slots
  } = use(DropdownContext);
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Popover, {
      ...props,
      className: composeTwRenderProps(className, slots?.popover()),
      "data-slot": "dropdown-popover",
      placement: placement,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Menu (Menu wrapper)
 * -----------------------------------------------------------------------------------------------*/

function DropdownMenu({
  className,
  ...props
}) {
  const {
    slots
  } = use(DropdownContext);
  return /*#__PURE__*/jsx(Menu, {
    className: composeTwRenderProps(className, slots?.menu()),
    "data-selection-mode": props.selectionMode,
    "data-slot": "dropdown-menu",
    ...props
  });
}

/* -------------------------------------------------------------------------------------------------
 * Dropdown Item (MenuItem wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownItem = props => {
  return /*#__PURE__*/jsx(MenuItemRoot, {
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Submenu Indicator (MenuItemSubmenuIndicator wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownSubmenuIndicator = props => {
  return /*#__PURE__*/jsx(MenuItemSubmenuIndicator, {
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Submenu Trigger
 * -----------------------------------------------------------------------------------------------*/

const DropdownSubmenuTrigger = ({
  children,
  ...props
}) => {
  return /*#__PURE__*/jsx(SubmenuTrigger, {
    "data-slot": "dropdown-submenu-trigger",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Item Indicator (MenuItemIndicator wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownItemIndicator = props => {
  return /*#__PURE__*/jsx(MenuItemIndicator, {
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Dropdown Section (MenuSection wrapper)
 * -----------------------------------------------------------------------------------------------*/

const DropdownSection = props => {
  return /*#__PURE__*/jsx(MenuSectionRoot, {
    ...props
  });
};

export { DropdownItem, DropdownItemIndicator, DropdownMenu, DropdownPopover, DropdownRoot, DropdownSection, DropdownSubmenuIndicator, DropdownSubmenuTrigger, DropdownTrigger };
