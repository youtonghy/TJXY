"use strict";
import { TabsRoot, TabPanel, TabSeparator, TabIndicator, Tab, TabList, TabListContainer } from './tabs.js';
export { tabsVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Tabs = Object.assign(TabsRoot, {
  Root: TabsRoot,
  ListContainer: TabListContainer,
  List: TabList,
  Tab,
  Indicator: TabIndicator,
  Separator: TabSeparator,
  Panel: TabPanel
});

export { Tab, TabIndicator, TabList, TabListContainer, TabPanel, TabSeparator, Tabs, TabsRoot };
