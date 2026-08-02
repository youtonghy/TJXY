"use client";
import { meterVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Meter } from 'react-aria-components/Meter';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const MeterContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Meter Root
 * -----------------------------------------------------------------------------------------------*/

const MeterRoot = ({
  children,
  className,
  color,
  size,
  ...props
}) => {
  const slots = React__default.useMemo(() => meterVariants({
    color,
    size
  }), [color, size]);
  return /*#__PURE__*/jsx(Meter, {
    "data-slot": "meter",
    ...props,
    className: composeTwRenderProps(className, slots.base()),
    children: values => /*#__PURE__*/jsx(MeterContext, {
      value: {
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};
MeterRoot.displayName = "HeroUI.Meter";

/* -------------------------------------------------------------------------------------------------
 * Meter Output
 * -----------------------------------------------------------------------------------------------*/

const MeterOutput = ({
  children,
  className,
  ...props
}) => {
  const {
    slots,
    state
  } = use(MeterContext);
  return /*#__PURE__*/jsx(dom.span, {
    className: composeSlotClassName(slots?.output, className),
    "data-slot": "meter-output",
    ...props,
    children: children ?? state?.valueText
  });
};
MeterOutput.displayName = "HeroUI.Meter.Output";

/* -------------------------------------------------------------------------------------------------
 * Meter Track
 * -----------------------------------------------------------------------------------------------*/

const MeterTrack = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(MeterContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.track, className),
    "data-slot": "meter-track",
    ...props,
    children: children
  });
};
MeterTrack.displayName = "HeroUI.Meter.Track";

/* -------------------------------------------------------------------------------------------------
 * Meter Fill
 * -----------------------------------------------------------------------------------------------*/

const MeterFill = ({
  className,
  style,
  ...props
}) => {
  const {
    slots,
    state
  } = use(MeterContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.fill, className),
    "data-slot": "meter-fill",
    style: {
      ...style,
      width: `${state?.percentage ?? 0}%`
    },
    ...props
  });
};
MeterFill.displayName = "HeroUI.Meter.Fill";

export { MeterFill, MeterOutput, MeterRoot, MeterTrack };
