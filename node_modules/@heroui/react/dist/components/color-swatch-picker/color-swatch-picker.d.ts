import type { DOMRenderProps } from "../../utils/dom";
import type { ColorSwatchPickerVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { ColorSwatchPickerItemRenderProps } from "react-aria-components/ColorSwatchPicker";
import React from "react";
import { ColorSwatchPickerItem as ColorSwatchPickerItemPrimitive, ColorSwatchPicker as ColorSwatchPickerPrimitive, ColorSwatch as ColorSwatchPrimitive } from "react-aria-components/ColorSwatchPicker";
interface ColorSwatchPickerRootProps extends ComponentPropsWithRef<typeof ColorSwatchPickerPrimitive>, ColorSwatchPickerVariants {
}
declare const ColorSwatchPickerRoot: ({ children, className, layout, size, variant, ...props }: ColorSwatchPickerRootProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSwatchPickerItemProps extends ComponentPropsWithRef<typeof ColorSwatchPickerItemPrimitive> {
}
declare const ColorSwatchPickerItem: ({ children, className, ...props }: ColorSwatchPickerItemProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSwatchPickerSwatchProps extends ComponentPropsWithRef<typeof ColorSwatchPrimitive> {
}
declare const ColorSwatchPickerSwatch: ({ className, ...props }: ColorSwatchPickerSwatchProps) => import("react/jsx-runtime").JSX.Element;
interface ColorSwatchPickerIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode | ((props: ColorSwatchPickerItemRenderProps) => React.ReactNode);
    className?: string;
}
declare const ColorSwatchPickerIndicator: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: ColorSwatchPickerIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ColorSwatchPickerIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { ColorSwatchPickerRoot, ColorSwatchPickerItem, ColorSwatchPickerSwatch, ColorSwatchPickerIndicator, };
export type { ColorSwatchPickerRootProps, ColorSwatchPickerItemProps, ColorSwatchPickerSwatchProps, ColorSwatchPickerIndicatorProps, };
