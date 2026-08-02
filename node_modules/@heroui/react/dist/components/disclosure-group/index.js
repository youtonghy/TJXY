"use strict";
import { DisclosureGroupRoot } from './disclosure-group.js';
export { disclosureGroupVariants } from '@heroui/styles';
export { useDisclosureGroupNavigation } from './use-disclosure-group-navigation.js';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const DisclosureGroup = Object.assign(DisclosureGroupRoot, {
  Root: DisclosureGroupRoot
});

export { DisclosureGroup, DisclosureGroupRoot };
