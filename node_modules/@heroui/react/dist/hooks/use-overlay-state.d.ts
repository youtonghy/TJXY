export interface UseOverlayStateProps {
    /**
     * Whether the overlay is currently open (controlled)
     */
    isOpen?: boolean;
    /**
     * Whether the overlay is open by default (uncontrolled)
     * @default false
     */
    defaultOpen?: boolean;
    /**
     * Handler that is called when the overlay's open state changes
     */
    onOpenChange?: (isOpen: boolean) => void;
}
export interface UseOverlayStateReturn {
    /**
     * Whether the overlay is currently open
     */
    readonly isOpen: boolean;
    /**
     * Sets the overlay's open state
     */
    setOpen(isOpen: boolean): void;
    /**
     * Opens the overlay
     */
    open(): void;
    /**
     * Closes the overlay
     */
    close(): void;
    /**
     * Toggles the overlay's open state
     */
    toggle(): void;
}
/**
 * Hook to manage overlay trigger state (Modal, AlertDialog, Popover, etc.)
 * Provides methods to open, close, and toggle the overlay
 *
 * @example
 * ```tsx
 * const state = useOverlayState();
 *
 * <Button onPress={state.open}>Open Dialog</Button>
 * <Modal isOpen={state.isOpen} onOpenChange={state.setOpen}>
 *   <Button onPress={state.close}>Close</Button>
 * </Modal>
 * ```
 */
export declare const useOverlayState: (props?: UseOverlayStateProps) => UseOverlayStateReturn;
