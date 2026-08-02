"use client";
import { numberFieldVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { Group } from 'react-aria-components/Group';
import { Input } from 'react-aria-components/Input';
import { NumberField } from 'react-aria-components/NumberField';
import { composeTwRenderProps } from '../../utils/compose.js';
import { IconMinus, IconPlus } from '../icons.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const NumberFieldContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * NumberField Root
 * -----------------------------------------------------------------------------------------------*/

const NumberFieldRoot = ({
  children,
  className,
  fullWidth,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => numberFieldVariants({
    fullWidth,
    variant
  }), [fullWidth, variant]);
  return /*#__PURE__*/jsx(NumberFieldContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(NumberField, {
      "data-slot": "number-field",
      ...props,
      className: composeTwRenderProps(className, slots?.base()),
      children: values => /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * NumberField Group
 * -----------------------------------------------------------------------------------------------*/

const NumberFieldGroup = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(NumberFieldContext);
  return /*#__PURE__*/jsx(Group, {
    className: composeTwRenderProps(className, slots?.group()),
    "data-slot": "number-field-group",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * NumberField Input
 * -----------------------------------------------------------------------------------------------*/

const NumberFieldInput = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(NumberFieldContext);
  return /*#__PURE__*/jsx(Input, {
    className: composeTwRenderProps(className, slots?.input()),
    "data-slot": "number-field-input",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * NumberField Increment Button
 * -----------------------------------------------------------------------------------------------*/

const NumberFieldIncrementButton = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(NumberFieldContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.incrementButton()),
    "data-slot": "number-field-increment-button",
    slot: "increment",
    ...props,
    children: children && /*#__PURE__*/React__default.isValidElement(children) ? children : /*#__PURE__*/jsx(IconPlus, {
      "data-slot": "number-field-increment-button-icon"
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * NumberField Decrement Button
 * -----------------------------------------------------------------------------------------------*/

const NumberFieldDecrementButton = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(NumberFieldContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.decrementButton()),
    "data-slot": "number-field-decrement-button",
    slot: "decrement",
    ...props,
    children: children && /*#__PURE__*/React__default.isValidElement(children) ? children : /*#__PURE__*/jsx(IconMinus, {
      "data-slot": "number-field-decrement-button-icon"
    })
  });
};

export { NumberFieldDecrementButton, NumberFieldGroup, NumberFieldIncrementButton, NumberFieldInput, NumberFieldRoot };
