"use strict";
import { useMemo, useCallback } from 'react';

function useDisclosureGroupNavigation({
  allowsMultipleExpanded = false,
  expandedKeys,
  itemIds = [],
  onExpandedChange
}) {
  const currentIndex = useMemo(() => {
    const expandedItems = itemIds.filter(id => expandedKeys.has(id));
    const currentItem = expandedItems.length > 0 ? expandedItems[0] : itemIds[0];
    if (!currentItem) return -1;
    return itemIds.indexOf(currentItem);
  }, [expandedKeys, itemIds]);
  const handlePrevious = useCallback(() => {
    if (currentIndex <= 0) return;
    const prevItem = itemIds[currentIndex - 1];
    if (!prevItem) return;
    if (allowsMultipleExpanded) {
      const newKeys = new Set(expandedKeys);
      newKeys.add(prevItem);
      onExpandedChange(newKeys);
    } else {
      onExpandedChange(new Set([prevItem]));
    }
  }, [currentIndex, itemIds, expandedKeys, onExpandedChange, allowsMultipleExpanded]);
  const handleNext = useCallback(() => {
    if (currentIndex >= itemIds.length - 1) return;
    const nextItem = itemIds[currentIndex + 1];
    if (!nextItem) return;
    if (allowsMultipleExpanded) {
      const newKeys = new Set(expandedKeys);
      newKeys.add(nextItem);
      onExpandedChange(newKeys);
    } else {
      onExpandedChange(new Set([nextItem]));
    }
  }, [currentIndex, itemIds.length, itemIds, expandedKeys, onExpandedChange, allowsMultipleExpanded]);
  const isPrevDisabled = currentIndex <= 0;
  const isNextDisabled = currentIndex >= itemIds.length - 1;
  return {
    currentIndex,
    isPrevDisabled,
    isNextDisabled,
    onPrevious: handlePrevious,
    onNext: handleNext
  };
}

export { useDisclosureGroupNavigation };
