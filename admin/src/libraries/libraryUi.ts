import type {
  ExpansionPolicy,
  LibraryCollectionType,
  MetadataPolicy,
  ObjectSelectionScope,
  ProbePolicy,
  ScanProfile,
} from './libraryApi';

export const collectionOptions: readonly { value: LibraryCollectionType; label: string }[] = [
  { value: 'mixed', label: 'Mixed content' },
  { value: 'movies', label: 'Movies' },
  { value: 'tvshows', label: 'TV shows' },
  { value: 'music', label: 'Music' },
  { value: 'homevideos', label: 'Home videos' },
];

export const scanProfileOptions: readonly { value: ScanProfile; label: string }[] = [
  { value: 'Full', label: 'Full' },
  { value: 'Lazy', label: 'Lazy' },
  { value: 'Hybrid', label: 'Hybrid' },
  { value: 'Manual', label: 'Manual' },
];

export const objectScopeOptions: readonly { value: ObjectSelectionScope; label: string }[] = [
  { value: 'all_synced_objects', label: 'All synced objects' },
  { value: 'title_layer', label: 'Title layer' },
  { value: 'library_roots', label: 'Library roots' },
];

export const metadataPolicyOptions: readonly { value: MetadataPolicy; label: string }[] = [
  { value: 'full', label: 'Full metadata' },
  { value: 'basic', label: 'Basic metadata' },
  { value: 'none', label: 'No metadata' },
];

export const expansionPolicyOptions: readonly { value: ExpansionPolicy; label: string }[] = [
  { value: 'eager', label: 'Eager' },
  { value: 'on_browse', label: 'On browse' },
  { value: 'background', label: 'Background' },
  { value: 'manual', label: 'Manual' },
];

export const probePolicyOptions: readonly { value: ProbePolicy; label: string }[] = [
  { value: 'eager', label: 'Eager' },
  { value: 'on_playback', label: 'On playback' },
  { value: 'manual', label: 'Manual' },
];

export function collectionLabel(value: string): string {
  return collectionOptions.find((option) => option.value === value)?.label ?? humanizeIdentifier(value);
}

export function optionLabel<T extends string>(
  options: readonly { value: T; label: string }[],
  value: T,
): string {
  return options.find((option) => option.value === value)?.label ?? humanizeIdentifier(value);
}

export function humanizeIdentifier(value: string): string {
  const spaced = value
    .replace(/([a-z0-9])([A-Z])/gu, '$1 $2')
    .replace(/[_-]+/gu, ' ')
    .replace(/\s+/gu, ' ')
    .trim();
  return spaced.length === 0 ? value : `${spaced.charAt(0).toUpperCase()}${spaced.slice(1)}`;
}
