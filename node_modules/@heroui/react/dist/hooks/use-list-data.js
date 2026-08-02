"use strict";
import { useState, useMemo } from 'react';

/*
 * Copyright 2020 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing permissions and limitations under the License.
 *
 * This file is based on the useListData hook from @react-stately/data package.
 * Original source: https://github.com/adobe/react-spectrum/blob/main/packages/%40react-stately/data/src/useListData.ts
 *
 * Why we copied this code instead of using @react-stately/data as a dependency:
 * We copied this implementation to avoid adding @react-stately/data as a dependency, which is a
 * large package that would significantly increase our bundle size. We maintain full attribution to
 * Adobe and comply with the Apache License 2.0 requirements.
 */
/**
 * Manages state for an immutable list data structure, and provides convenience methods to
 * update the data over time.
 */
function useListData(options) {
  const {
    filter,
    getKey = item => item.id ?? item.key,
    initialFilterText = "",
    initialItems = [],
    initialSelectedKeys
  } = options;

  // Store both items and filteredItems in state so we can go back to the unfiltered list
  const [state, setState] = useState({
    items: initialItems,
    selectedKeys: initialSelectedKeys === "all" ? "all" : new Set(initialSelectedKeys || []),
    filterText: initialFilterText
  });
  const filteredItems = useMemo(() => filter ? state.items.filter(item => filter(item, state.filterText)) : state.items, [state.items, state.filterText, filter]);
  return {
    ...state,
    items: filteredItems,
    ...createListActions({
      getKey
    }, setState),
    getItem(key) {
      return state.items.find(item => getKey(item) === key);
    }
  };
}
function createListActions(opts, dispatch) {
  const {
    cursor,
    getKey
  } = opts;
  return {
    setSelectedKeys(selectedKeys) {
      dispatch(state => ({
        ...state,
        selectedKeys
      }));
    },
    addKeysToSelection(selectedKeys) {
      dispatch(state => {
        if (state.selectedKeys === "all") {
          return state;
        }
        if (selectedKeys === "all") {
          return {
            ...state,
            selectedKeys: "all"
          };
        }
        return {
          ...state,
          selectedKeys: new Set([...state.selectedKeys, ...selectedKeys])
        };
      });
    },
    removeKeysFromSelection(selectedKeys) {
      dispatch(state => {
        if (selectedKeys === "all") {
          return {
            ...state,
            selectedKeys: new Set()
          };
        }
        const selection = state.selectedKeys === "all" ? new Set(state.items.map(getKey)) : new Set(state.selectedKeys);
        for (const key of selectedKeys) {
          selection.delete(key);
        }
        return {
          ...state,
          selectedKeys: selection
        };
      });
    },
    setFilterText(filterText) {
      dispatch(state => ({
        ...state,
        filterText
      }));
    },
    insert(index, ...values) {
      dispatch(state => insert(state, index, ...values));
    },
    insertBefore(key, ...values) {
      dispatch(state => {
        let index = state.items.findIndex(item => getKey?.(item) === key);
        if (index === -1) {
          if (state.items.length === 0) {
            index = 0;
          } else {
            return state;
          }
        }
        return insert(state, index, ...values);
      });
    },
    insertAfter(key, ...values) {
      dispatch(state => {
        let index = state.items.findIndex(item => getKey?.(item) === key);
        if (index === -1) {
          if (state.items.length === 0) {
            index = 0;
          } else {
            return state;
          }
        }
        return insert(state, index + 1, ...values);
      });
    },
    prepend(...values) {
      dispatch(state => insert(state, 0, ...values));
    },
    append(...values) {
      dispatch(state => insert(state, state.items.length, ...values));
    },
    remove(...keys) {
      dispatch(state => {
        const keySet = new Set(keys);
        const items = state.items.filter(item => !keySet.has(getKey(item)));
        let selection = "all";
        if (state.selectedKeys !== "all") {
          selection = new Set(state.selectedKeys);
          for (const key of keys) {
            selection.delete(key);
          }
        }
        if (cursor == null && items.length === 0) {
          selection = new Set();
        }
        return {
          ...state,
          items,
          selectedKeys: selection
        };
      });
    },
    removeSelectedItems() {
      dispatch(state => {
        if (state.selectedKeys === "all") {
          return {
            ...state,
            items: [],
            selectedKeys: new Set()
          };
        }
        const selectedKeys = state.selectedKeys;
        const items = state.items.filter(item => !selectedKeys.has(getKey(item)));
        return {
          ...state,
          items,
          selectedKeys: new Set()
        };
      });
    },
    move(key, toIndex) {
      dispatch(state => {
        const index = state.items.findIndex(item => getKey(item) === key);
        if (index === -1) {
          return state;
        }
        const copy = state.items.slice();
        const [item] = copy.splice(index, 1);
        if (item !== undefined) {
          copy.splice(toIndex, 0, item);
        }
        return {
          ...state,
          items: copy
        };
      });
    },
    moveBefore(key, keys) {
      dispatch(state => {
        const toIndex = state.items.findIndex(item => getKey(item) === key);
        if (toIndex === -1) {
          return state;
        }

        // Find indices of keys to move. Sort them so that the order in the list is retained.
        const keyArray = Array.isArray(keys) ? keys : [...keys];
        const indices = keyArray.map(key => state.items.findIndex(item => getKey(item) === key)).sort((a, b) => a - b);
        return move(state, indices, toIndex);
      });
    },
    moveAfter(key, keys) {
      dispatch(state => {
        const toIndex = state.items.findIndex(item => getKey(item) === key);
        if (toIndex === -1) {
          return state;
        }
        const keyArray = Array.isArray(keys) ? keys : [...keys];
        const indices = keyArray.map(key => state.items.findIndex(item => getKey(item) === key)).sort((a, b) => a - b);
        return move(state, indices, toIndex + 1);
      });
    },
    update(key, newValue) {
      dispatch(state => {
        const index = state.items.findIndex(item => getKey(item) === key);
        if (index === -1) {
          return state;
        }
        const currentItem = state.items[index];
        if (currentItem === undefined) {
          return state;
        }
        let updatedValue;
        if (typeof newValue === "function") {
          updatedValue = newValue(currentItem);
        } else {
          updatedValue = newValue;
        }
        return {
          ...state,
          items: [...state.items.slice(0, index), updatedValue, ...state.items.slice(index + 1)]
        };
      });
    }
  };
}
function insert(state, index, ...values) {
  return {
    ...state,
    items: [...state.items.slice(0, index), ...values, ...state.items.slice(index)]
  };
}
function move(state, indices, toIndex) {
  // Shift the target down by the number of items being moved from before the target
  toIndex -= indices.filter(index => index < toIndex).length;
  const moves = indices.map(from => ({
    from,
    to: toIndex++
  }));

  // Shift later from indices down if they have a larger index
  for (let i = 0; i < moves.length; i++) {
    const a = moves[i];
    for (let j = i; j < moves.length; j++) {
      const b = moves[j];
      if (b.from > a.from) {
        b.from--;
      }
    }
  }

  // Interleave the moves so they can be applied one by one rather than all at once
  for (let i = 0; i < moves.length; i++) {
    const a = moves[i];
    for (let j = moves.length - 1; j > i; j--) {
      const b = moves[j];
      if (b.from < a.to) {
        a.to++;
      } else {
        b.from++;
      }
    }
  }
  const copy = state.items.slice();
  for (const moveItem of moves) {
    const [item] = copy.splice(moveItem.from, 1);
    if (item !== undefined) {
      copy.splice(moveItem.to, 0, item);
    }
  }
  return {
    ...state,
    items: copy
  };
}

export { createListActions, useListData };
