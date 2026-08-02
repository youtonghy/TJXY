"use client";
import { colorInputGroupVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Group } from 'react-aria-components/Group';
import { Input } from 'react-aria-components/Input';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const ColorInputGroupContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ColorInputGroup Root
 * -----------------------------------------------------------------------------------------------*/

const ColorInputGroupRoot = ({
  children,
  className,
  fullWidth,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => colorInputGroupVariants({
    fullWidth,
    variant
  }), [fullWidth, variant]);
  return /*#__PURE__*/jsx(ColorInputGroupContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(Group, {
      className: composeTwRenderProps(className, slots?.base()),
      "data-slot": "color-input-group",
      ...props,
      children: values => /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorInputGroup Prefix
 * -----------------------------------------------------------------------------------------------*/

const ColorInputGroupPrefix = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorInputGroupContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.prefix, className),
    "data-slot": "color-input-group-prefix",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorInputGroup Input
 * -----------------------------------------------------------------------------------------------*/

const ColorInputGroupInput = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorInputGroupContext);
  return /*#__PURE__*/jsx(Input, {
    className: composeTwRenderProps(className, slots?.input()),
    "data-slot": "color-input-group-input",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorInputGroup Suffix
 * -----------------------------------------------------------------------------------------------*/

const ColorInputGroupSuffix = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorInputGroupContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.suffix, className),
    "data-slot": "color-input-group-suffix",
    ...props,
    children: children
  });
};

export { ColorInputGroupInput, ColorInputGroupPrefix, ColorInputGroupRoot, ColorInputGroupSuffix };
