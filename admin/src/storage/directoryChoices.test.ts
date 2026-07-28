import { uniqueChoices } from './directoryChoices';

it('keeps first-seen folder order while removing duplicate identifiers', () => {
  expect(uniqueChoices([
    { id: 'folder-1', name: 'Shows' },
    { id: 'folder-1', name: 'Renamed duplicate' },
    { id: 'folder-2', name: 'Archive' },
  ])).toEqual([
    { id: 'folder-1', name: 'Shows' },
    { id: 'folder-2', name: 'Archive' },
  ]);
});
