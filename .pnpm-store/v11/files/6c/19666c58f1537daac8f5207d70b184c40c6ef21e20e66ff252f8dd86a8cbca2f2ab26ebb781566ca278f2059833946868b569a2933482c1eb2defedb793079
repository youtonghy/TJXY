"use client";
import { progressCircleVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ProgressBar } from 'react-aria-components/ProgressBar';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const STROKE_WIDTH = 4;
const CENTER = 18;
const RADIUS = CENTER - STROKE_WIDTH / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/* -------------------------------------------------------------------------------------------------
 * ProgressCircle Context
 * -----------------------------------------------------------------------------------------------*/

const ProgressCircleContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ProgressCircle Root
 * -----------------------------------------------------------------------------------------------*/

const ProgressCircleRoot = ({
  children,
  className,
  color,
  size,
  ...props
}) => {
  const slots = React__default.useMemo(() => progressCircleVariants({
    color,
    size
  }), [color, size]);
  return /*#__PURE__*/jsx(ProgressBar, {
    "data-slot": "progress-circle",
    ...props,
    className: composeTwRenderProps(className, slots.base()),
    children: values => /*#__PURE__*/jsx(ProgressCircleContext, {
      value: {
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};
ProgressCircleRoot.displayName = "HeroUI.ProgressCircle";

/* -------------------------------------------------------------------------------------------------
 * ProgressCircle Track
 * -----------------------------------------------------------------------------------------------*/

const ProgressCircleTrack = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ProgressCircleContext);
  return /*#__PURE__*/jsx(dom.svg, {
    className: composeSlotClassName(slots?.track, className),
    "data-slot": "progress-circle-track",
    fill: "none",
    viewBox: `0 0 ${CENTER * 2} ${CENTER * 2}`,
    ...props,
    children: children
  });
};
ProgressCircleTrack.displayName = "HeroUI.ProgressCircle.Track";

/* -------------------------------------------------------------------------------------------------
 * ProgressCircle TrackCircle
 * -----------------------------------------------------------------------------------------------*/

const ProgressCircleTrackCircle = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(ProgressCircleContext);
  return /*#__PURE__*/jsx(dom.circle, {
    className: composeSlotClassName(slots?.trackCircle, className),
    cx: CENTER,
    cy: CENTER,
    "data-slot": "progress-circle-track-circle",
    r: RADIUS,
    strokeWidth: STROKE_WIDTH,
    ...props
  });
};
ProgressCircleTrackCircle.displayName = "HeroUI.ProgressCircle.TrackCircle";

/* -------------------------------------------------------------------------------------------------
 * ProgressCircle FillCircle
 * -----------------------------------------------------------------------------------------------*/

const ProgressCircleFillCircle = ({
  className,
  ...props
}) => {
  const {
    slots,
    state
  } = use(ProgressCircleContext);
  const percentage = state?.percentage ?? 0;
  const isIndeterminate = state?.isIndeterminate ?? false;
  const strokeDashoffset = CIRCUMFERENCE - percentage / 100 * CIRCUMFERENCE;
  return /*#__PURE__*/jsx(dom.circle, {
    className: composeSlotClassName(slots?.fillCircle, className),
    cx: CENTER,
    cy: CENTER,
    "data-slot": "progress-circle-fill-circle",
    r: RADIUS,
    strokeDasharray: CIRCUMFERENCE,
    strokeDashoffset: isIndeterminate ? CIRCUMFERENCE * 0.75 : strokeDashoffset,
    strokeLinecap: "round",
    strokeWidth: STROKE_WIDTH,
    transform: `rotate(-90 ${CENTER} ${CENTER})`,
    ...props
  });
};
ProgressCircleFillCircle.displayName = "HeroUI.ProgressCircle.FillCircle";

export { ProgressCircleFillCircle, ProgressCircleRoot, ProgressCircleTrack, ProgressCircleTrackCircle };
