"use client";
import { colorSliderVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ColorSlider, ColorThumb, SliderTrack, SliderOutput } from 'react-aria-components/ColorSlider';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Validation Utilities
 * -----------------------------------------------------------------------------------------------*/

/** Maps channels to their required color space (for channels that are color-space specific) */
const CHANNEL_TO_REQUIRED_COLORSPACE = {
  red: "rgb",
  green: "rgb",
  blue: "rgb",
  lightness: "hsl",
  brightness: "hsb"
};

/** Channels that only work with HSL or HSB (not RGB) */
const HSL_HSB_ONLY_CHANNELS = new Set(["hue", "saturation"]);

/**
 * Validates and returns a valid colorSpace for the given channel.
 * If an invalid combination is detected, logs a warning and returns the correct colorSpace.
 */
function getValidColorSpace(channel, colorSpace) {
  // Check if channel requires a specific color space (e.g., "red" requires "rgb")
  const requiredSpace = CHANNEL_TO_REQUIRED_COLORSPACE[channel];
  if (requiredSpace && colorSpace && colorSpace !== requiredSpace) {
    // eslint-disable-next-line no-console
    console.warn(`[HeroUI ColorSlider] Invalid combination: channel="${channel}" requires colorSpace="${requiredSpace}", ` + `but received colorSpace="${colorSpace}". Auto-correcting to "${requiredSpace}".`);
    return requiredSpace;
  }

  // Check if channel is HSL/HSB only (hue, saturation) but RGB was specified
  if (HSL_HSB_ONLY_CHANNELS.has(channel) && colorSpace === "rgb") {
    // eslint-disable-next-line no-console
    console.warn(`[HeroUI ColorSlider] Invalid combination: channel="${channel}" is not available in RGB color space. ` + `Use colorSpace="hsl" or colorSpace="hsb" instead. Auto-correcting to "hsl".`);
    return "hsl";
  }
  return colorSpace;
}

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Context
 * -----------------------------------------------------------------------------------------------*/

const ColorSliderContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Root
 * -----------------------------------------------------------------------------------------------*/

const ColorSliderRoot = ({
  channel,
  children,
  className,
  colorSpace,
  orientation = "horizontal",
  ...props
}) => {
  const slots = React__default.useMemo(() => colorSliderVariants({}), []);

  // Validate and auto-correct invalid channel/colorSpace combinations
  const validColorSpace = getValidColorSpace(channel, colorSpace);
  return /*#__PURE__*/jsx(ColorSlider, {
    channel: channel,
    colorSpace: validColorSpace,
    "data-slot": "color-slider",
    orientation: orientation,
    ...props,
    className: composeTwRenderProps(className, slots.base()),
    children: values => /*#__PURE__*/jsx(ColorSliderContext, {
      value: {
        channel,
        slots,
        state: values
      },
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Output
 * -----------------------------------------------------------------------------------------------*/

const ColorSliderOutput = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorSliderContext);
  return /*#__PURE__*/jsx(SliderOutput, {
    className: composeTwRenderProps(className, slots?.output()),
    "data-slot": "color-slider-output",
    ...props,
    children: children ? values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    }) : ({
      state
    }) => state.getThumbValueLabel(0)
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Track
 * -----------------------------------------------------------------------------------------------*/

const ColorSliderTrack = ({
  children,
  className,
  style,
  ...props
}) => {
  const {
    channel,
    slots,
    state
  } = use(ColorSliderContext);
  // Calculate start and end colors for the gradient edge caps
  const displayColor = state?.state?.getDisplayColor();
  const edgeColors = React__default.useMemo(() => {
    // Access color through state.state.value (ColorSliderState.value)
    if (!displayColor || !channel) {
      return {
        end: "transparent",
        start: "transparent"
      };
    }
    const range = displayColor.getChannelRange(channel);

    // Get colors at min and max values of the channel
    const startColor = displayColor.withChannelValue(channel, range.minValue);
    const endColor = displayColor.withChannelValue(channel, range.maxValue);
    return {
      end: endColor.toString("css"),
      start: startColor.toString("css")
    };
  }, [channel, displayColor]);
  return /*#__PURE__*/jsx(SliderTrack, {
    className: composeTwRenderProps(className, slots?.track()),
    "data-slot": "color-slider-track",
    style: ({
      defaultStyle,
      ...rest
    }) => ({
      // Add transparency checkerboard pattern for alpha channel
      background: `${defaultStyle.background}, repeating-conic-gradient(#efefef 0% 25%, #f7f7f7 0% 50%) 50% / 16px 16px`,
      // Pass edge colors as CSS custom properties for ::before and ::after
      "--track-end-color": edgeColors.end,
      "--track-start-color": edgeColors.start,
      ...(typeof style === "function" ? style({
        defaultStyle,
        ...rest
      }) : style)
    }),
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorSlider Thumb
 * -----------------------------------------------------------------------------------------------*/

const ColorSliderThumb = ({
  children,
  className,
  style,
  ...props
}) => {
  const {
    slots
  } = use(ColorSliderContext);
  return /*#__PURE__*/jsx(ColorThumb, {
    className: composeTwRenderProps(className, slots?.thumb()),
    "data-slot": "color-slider-thumb",
    style: ({
      defaultStyle,
      isDisabled,
      ...rest
    }) => ({
      ...defaultStyle,
      backgroundColor: isDisabled ? undefined : defaultStyle.backgroundColor,
      ...(typeof style === "function" ? style({
        defaultStyle,
        isDisabled,
        ...rest
      }) : style)
    }),
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

export { ColorSliderOutput, ColorSliderRoot, ColorSliderThumb, ColorSliderTrack };
