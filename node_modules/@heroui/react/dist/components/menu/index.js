"use strict";
import { MenuItem } from '../menu-item/index.js';
import { MenuSection } from '../menu-section/index.js';
import { MenuRoot } from './menu.js';
export { menuVariants } from '@heroui/styles';
import { MenuItemIndicator } from '../menu-item/menu-item.js';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Menu = Object.assign(MenuRoot, {
  Root: MenuRoot,
  Item: MenuItem,
  ItemIndicator: MenuItemIndicator,
  Section: MenuSection
});

export { Menu, MenuRoot };
