"use strict";
import { ColorPickerRoot, ColorPickerPopover, ColorPickerTrigger } from './color-picker.js';
export { colorPickerVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ColorPicker = Object.assign(ColorPickerRoot, {
  Root: ColorPickerRoot,
  Trigger: ColorPickerTrigger,
  Popover: ColorPickerPopover
});

export { ColorPicker, ColorPickerPopover, ColorPickerRoot, ColorPickerTrigger };
