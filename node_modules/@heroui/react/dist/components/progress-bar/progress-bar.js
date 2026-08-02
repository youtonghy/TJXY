"use client";
import { progressBarVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ProgressBar } from 'react-aria-components/ProgressBar';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const ProgressBarContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ProgressBar Root
 * -----------------------------------------------------------------------------------------------*/

const ProgressBarRoot = ({
  children,
  className,
  color,
  size,
  ...props
}) => {
  const slots = React__default.useMemo(() => progressBarVariants({
    color,
    size
  }), [color, size]);
  return /*#__PURE__*/jsx(ProgressBar, {
    "data-slot": "progress-bar",
    ...props,
    className: composeTwRenderProps(className, slots.base()),
    children: values => /*#__PURE__*/jsx(ProgressBarContext, {
      value: {
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};
ProgressBarRoot.displayName = "HeroUI.ProgressBar";

/* -------------------------------------------------------------------------------------------------
 * ProgressBar Output
 * -----------------------------------------------------------------------------------------------*/

const ProgressBarOutput = ({
  children,
  className,
  ...props
}) => {
  const {
    slots,
    state
  } = use(ProgressBarContext);
  return /*#__PURE__*/jsx(dom.span, {
    className: composeSlotClassName(slots?.output, className),
    "data-slot": "progress-bar-output",
    ...props,
    children: children ?? state?.valueText
  });
};
ProgressBarOutput.displayName = "HeroUI.ProgressBar.Output";

/* -------------------------------------------------------------------------------------------------
 * ProgressBar Track
 * -----------------------------------------------------------------------------------------------*/

const ProgressBarTrack = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ProgressBarContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.track, className),
    "data-slot": "progress-bar-track",
    ...props,
    children: children
  });
};
ProgressBarTrack.displayName = "HeroUI.ProgressBar.Track";

/* -------------------------------------------------------------------------------------------------
 * ProgressBar Fill
 * -----------------------------------------------------------------------------------------------*/

const ProgressBarFill = ({
  className,
  style,
  ...props
}) => {
  const {
    slots,
    state
  } = use(ProgressBarContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.fill, className),
    "data-slot": "progress-bar-fill",
    style: {
      ...style,
      width: state?.isIndeterminate ? undefined : `${state?.percentage ?? 0}%`
    },
    ...props
  });
};
ProgressBarFill.displayName = "HeroUI.ProgressBar.Fill";

export { ProgressBarFill, ProgressBarOutput, ProgressBarRoot, ProgressBarTrack };
