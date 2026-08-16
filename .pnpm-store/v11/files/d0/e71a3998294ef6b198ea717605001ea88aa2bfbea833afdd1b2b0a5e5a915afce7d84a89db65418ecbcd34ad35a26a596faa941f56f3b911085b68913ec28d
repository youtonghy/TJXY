/**
 * Collection roots (TagGroup, etc.) whose children are fed to RAC CollectionBuilder
 * mount a Hidden tree *before* TextContext slots exist. Wrap those roots with
 * `FieldSlotsGate` so Description / ErrorMessage can wait for the visible pass.
 */
declare const FieldSlotsGateContext: import("react").Context<boolean>;
/**
 * Whether RAC `TextContext.slots[slot]` is available for rendering.
 * Outside a `FieldSlotsGate`, always `true` (standalone / TextField / etc.).
 * Inside a gate, `true` only after the collection root's visible Provider mounts.
 */
declare const useHasTextSlot: (slot: string) => boolean;
export { FieldSlotsGateContext, useHasTextSlot };
