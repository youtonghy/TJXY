import type { ColorSliderVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { ColorSpace } from "react-aria-components/ColorSlider";
import { ColorSlider as ColorSliderPrimitive, ColorThumb as ColorThumbPrimitive, SliderOutput as SliderOutputPrimitive, SliderTrack as SliderTrackPrimitive } from "react-aria-components/ColorSlider";
/** Channels available in HSL color space */
type HSLChannel = "hue" | "saturation" | "lightness" | "alpha";
/** Channels available in HSB color space */
type HSBChannel = "hue" | "saturation" | "brightness" | "alpha";
/** Channels available in RGB color space */
type RGBChannel = "red" | "green" | "blue" | "alpha";
/** Channels shared between HSL and HSB (but NOT RGB) */
type HSLHSBSharedChannel = "hue" | "saturation";
/** Alpha channel works across ALL color spaces */
type AlphaChannel = "alpha";
/**
 * Discriminated union type for valid channel/colorSpace combinations.
 * This ensures TypeScript will error on invalid combinations like
 * `channel="red"` with `colorSpace="hsl"` or `channel="saturation"` with `colorSpace="rgb"`.
 */
type ColorSliderChannelProps = {
    channel: HSLChannel;
    colorSpace?: "hsl";
} | {
    channel: HSBChannel;
    colorSpace?: "hsb";
} | {
    channel: RGBChannel;
    colorSpace?: "rgb";
} | {
    channel: HSLHSBSharedChannel;
    colorSpace?: "hsl" | "hsb";
} | {
    channel: AlphaChannel;
    colorSpace?: ColorSpace;
};
interface ColorSliderRootBaseProps extends Omit<ComponentPropsWithRef<typeof ColorSliderPrimitive>, "channel" | "colorSpace">, ColorSliderVariants {
}
type ColorSliderRootProps = ColorSliderRootBaseProps & ColorSliderChannelProps;
declare const ColorSliderRoot: ({ channel, children, className, colorSpace, orientation, ...props }: ColorSliderRootProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSliderOutputProps extends ComponentPropsWithRef<typeof SliderOutputPrimitive> {
}
declare const ColorSliderOutput: ({ children, className, ...props }: ColorSliderOutputProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSliderTrackProps extends ComponentPropsWithRef<typeof SliderTrackPrimitive> {
}
declare const ColorSliderTrack: ({ children, className, style, ...props }: ColorSliderTrackProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSliderThumbProps extends ComponentPropsWithRef<typeof ColorThumbPrimitive> {
}
declare const ColorSliderThumb: ({ children, className, style, ...props }: ColorSliderThumbProps) => import("react/jsx-runtime").JSX.Element;
export { ColorSliderRoot, ColorSliderOutput, ColorSliderTrack, ColorSliderThumb };
export type { ColorSliderRootProps, ColorSliderOutputProps, ColorSliderTrackProps, ColorSliderThumbProps, HSLChannel, HSBChannel, RGBChannel, HSLHSBSharedChannel, AlphaChannel, ColorSliderChannelProps, };
