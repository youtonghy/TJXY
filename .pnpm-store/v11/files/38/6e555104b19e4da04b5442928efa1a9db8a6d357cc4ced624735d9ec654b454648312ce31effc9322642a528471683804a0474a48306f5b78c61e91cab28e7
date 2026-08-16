"use client";
import { dateInputGroupVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { DateSegment, DateInput } from 'react-aria-components/DateField';
import { Group } from 'react-aria-components/Group';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const DateInputGroupContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup Root
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupRoot = ({
  children,
  className,
  fullWidth,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => dateInputGroupVariants({
    fullWidth,
    variant
  }), [fullWidth, variant]);
  return /*#__PURE__*/jsx(DateInputGroupContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(Group, {
      className: composeTwRenderProps(className, slots?.base()),
      "data-slot": "date-input-group",
      ...props,
      children: values => /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup Prefix
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupPrefix = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DateInputGroupContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.prefix, className),
    "data-slot": "date-input-group-prefix",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup Input
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupInput = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(DateInputGroupContext);

  // TimeInput and DateInput have compatible interfaces
  // React Aria Components will handle the correct primitive based on parent context (TimeField vs DateField)
  // We use DateInputPrimitive as the default, but it will work with TimeField context
  return /*#__PURE__*/jsx(DateInput, {
    className: composeTwRenderProps(className, slots?.input()),
    "data-slot": "date-input-group-input",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup Segment
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupSegment = ({
  className,
  segment,
  ...props
}) => {
  const {
    slots
  } = use(DateInputGroupContext);

  // TimeSegment and DateSegment have compatible interfaces
  // React Aria Components will handle the correct primitive based on parent context
  // We use DateSegmentPrimitive as the default, but it will work with TimeField context
  return /*#__PURE__*/jsx(DateSegment, {
    className: composeSlotClassName(slots?.segment, className),
    "data-slot": "date-input-group-segment",
    segment: segment,
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup InputContainer
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupInputContainer = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DateInputGroupContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.inputContainer, className),
    "data-slot": "date-input-group-input-container",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * DateInputGroup Suffix
 * -----------------------------------------------------------------------------------------------*/

const DateInputGroupSuffix = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DateInputGroupContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.suffix, className),
    "data-slot": "date-input-group-suffix",
    ...props,
    children: children
  });
};

export { DateInputGroupInput, DateInputGroupInputContainer, DateInputGroupPrefix, DateInputGroupRoot, DateInputGroupSegment, DateInputGroupSuffix };
