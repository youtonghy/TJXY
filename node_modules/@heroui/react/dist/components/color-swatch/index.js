"use strict";
import { ColorSwatchRoot } from './color-swatch.js';
export { colorSwatchVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ColorSwatch = Object.assign(ColorSwatchRoot, {
  Root: ColorSwatchRoot
});

export { ColorSwatch, ColorSwatchRoot };
