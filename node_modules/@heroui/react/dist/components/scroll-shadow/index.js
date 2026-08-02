"use strict";
import { ScrollShadowRoot } from './scroll-shadow.js';
export { scrollShadowVariants } from '@heroui/styles';
export { useScrollShadow } from './use-scroll-shadow.js';

/* -------------------------------------------------------------------------------------------------
 * Component
 * -----------------------------------------------------------------------------------------------*/
const ScrollShadow = Object.assign(ScrollShadowRoot, {
  Root: ScrollShadowRoot
});

export { ScrollShadow, ScrollShadowRoot };
