export interface UseDisclosureGroupNavigationProps {
    expandedKeys: Set<string | number>;
    itemIds: string[];
    onExpandedChange: (keys: Set<string | number>) => void;
    allowsMultipleExpanded?: boolean;
}
export interface UseDisclosureGroupNavigationReturn {
    currentIndex: number;
    isPrevDisabled: boolean;
    isNextDisabled: boolean;
    onPrevious: () => void;
    onNext: () => void;
}
export declare function useDisclosureGroupNavigation({ allowsMultipleExpanded, expandedKeys, itemIds, onExpandedChange, }: UseDisclosureGroupNavigationProps): UseDisclosureGroupNavigationReturn;
