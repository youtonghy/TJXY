"use client";
import { chipVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const ChipContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Chip Root
 * -----------------------------------------------------------------------------------------------*/

const ChipRoot = ({
  children,
  className,
  color,
  size,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => chipVariants({
    color,
    size,
    variant
  }), [color, size, variant]);
  const chipChildren = React__default.useMemo(() => {
    if (typeof children === "string" || typeof children === "number") {
      return /*#__PURE__*/jsx(ChipLabel, {
        children: children
      });
    }
    return children;
  }, [children]);
  return /*#__PURE__*/jsx(ChipContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(dom.span, {
      ...props,
      className: composeSlotClassName(slots.base, className),
      "data-slot": "chip",
      children: chipChildren
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Chip Label
 * -----------------------------------------------------------------------------------------------*/

const ChipLabel = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ChipContext);
  return /*#__PURE__*/jsx(dom.span, {
    className: composeSlotClassName(slots?.label, className),
    "data-slot": "chip-label",
    ...props,
    children: children
  });
};

export { ChipLabel, ChipRoot };
