"use strict";
import { DisclosureRoot, DisclosureIndicator, DisclosureBody, DisclosureContent, DisclosureTrigger, DisclosureHeading } from './disclosure.js';
export { disclosureVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Disclosure = Object.assign(DisclosureRoot, {
  Root: DisclosureRoot,
  Heading: DisclosureHeading,
  Trigger: DisclosureTrigger,
  Content: DisclosureContent,
  Body: DisclosureBody,
  Indicator: DisclosureIndicator
});

export { Disclosure, DisclosureBody, DisclosureContent, DisclosureHeading, DisclosureIndicator, DisclosureRoot, DisclosureTrigger };
