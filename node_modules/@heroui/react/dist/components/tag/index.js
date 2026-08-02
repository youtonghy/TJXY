"use strict";
import { TagRoot, TagRemoveButton } from './tag.js';
export { tagVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Tag = Object.assign(TagRoot, {
  Root: TagRoot,
  RemoveButton: TagRemoveButton
});

export { Tag, TagRemoveButton, TagRoot };
