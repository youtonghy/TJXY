"use client";
import { listboxItemVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ListBoxItem } from 'react-aria-components/ListBox';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';

const ListBoxItemContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ListBox Item Root
 * -----------------------------------------------------------------------------------------------*/

const ListBoxItemRoot = ({
  children,
  className,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => listboxItemVariants({
    variant
  }), [variant]);
  return /*#__PURE__*/jsx(ListBoxItem, {
    className: composeTwRenderProps(className, slots.item()),
    "data-slot": "list-box-item",
    ...props,
    children: values => /*#__PURE__*/jsx(ListBoxItemContext, {
      value: {
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ListBox Item Indicator
 * -----------------------------------------------------------------------------------------------*/

const ListBoxItemIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots,
    state
  } = use(ListBoxItemContext);
  const isSelected = state?.isSelected;
  const content = typeof children === "function" ? children(state ?? {}) : children ? children : /*#__PURE__*/jsx("svg", {
    "aria-hidden": "true",
    "data-slot": "list-box-item-indicator--checkmark",
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
    "data-slot": "list-box-item-indicator",
    "data-visible": isSelected || undefined,
    ...props,
    children: content
  });
};

export { ListBoxItemIndicator, ListBoxItemRoot };
