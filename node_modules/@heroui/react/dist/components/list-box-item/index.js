"use strict";
import { ListBoxItemRoot, ListBoxItemIndicator } from './list-box-item.js';
export { listboxItemVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ListBoxItem = Object.assign(ListBoxItemRoot, {
  Root: ListBoxItemRoot,
  Indicator: ListBoxItemIndicator
});

export { ListBoxItem, ListBoxItemIndicator, ListBoxItemRoot };
