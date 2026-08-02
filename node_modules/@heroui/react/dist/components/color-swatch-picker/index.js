"use strict";
import { ColorSwatchPickerRoot, ColorSwatchPickerIndicator, ColorSwatchPickerSwatch, ColorSwatchPickerItem } from './color-swatch-picker.js';
export { colorSwatchPickerVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
| * Compound Component
| * -----------------------------------------------------------------------------------------------*/
const ColorSwatchPicker = Object.assign(ColorSwatchPickerRoot, {
  Root: ColorSwatchPickerRoot,
  Item: ColorSwatchPickerItem,
  Swatch: ColorSwatchPickerSwatch,
  Indicator: ColorSwatchPickerIndicator
});

export { ColorSwatchPicker, ColorSwatchPickerIndicator, ColorSwatchPickerItem, ColorSwatchPickerRoot, ColorSwatchPickerSwatch };
