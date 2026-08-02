import {useEvent as $c3cab330536504ec$export$90fc3a17d93f704c} from "../utils/useEvent.js";
import {useState as $bIBdX$useState, useRef as $bIBdX$useRef} from "react";

/*
 * Copyright 2026 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing
 */ 

function $a06747c3f56c51df$export$31dcd79391e423fe(// Unused but matches the normal signature.
_props, _state, ref) {
    let [isSelected, setSelected] = (0, $bIBdX$useState)(false);
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)((0, $bIBdX$useRef)(typeof document !== 'undefined' ? document : null), 'selectionchange', ()=>{
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0 || !ref.current) return;
        let range = selection.getRangeAt(0);
        if (!range.collapsed && range.intersectsNode(ref.current)) setSelected(true);
        else setSelected(false);
    });
    return {
        tokenProps: {
            contentEditable: false,
            suppressContentEditableWarning: true,
            style: {
                userSelect: 'all',
                WebkitUserSelect: 'all'
            }
        },
        isSelected: isSelected
    };
}


export {$a06747c3f56c51df$export$31dcd79391e423fe as useToken};
//# sourceMappingURL=useToken.js.map
