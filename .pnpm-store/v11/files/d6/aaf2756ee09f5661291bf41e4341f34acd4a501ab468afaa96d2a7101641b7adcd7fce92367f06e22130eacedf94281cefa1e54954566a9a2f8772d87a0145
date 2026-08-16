import {OverlayTriggerStateContext as $acf8e70c2f419f18$export$d2f961adcb0afbe} from "./Dialog.js";
import {PopoverContext as $03df2f2d3cffb62f$export$9b9a0cd73afb7ca4} from "./Popover.js";
import {Provider as $b7b7a92703138c9b$export$2881499e37b75b9a} from "./utils.js";
import {usePreviewTrigger as $gmnOE$usePreviewTrigger} from "react-aria/usePreviewTrigger";
import {FocusableProvider as $gmnOE$FocusableProvider} from "react-aria/private/interactions/useFocusable";
import $gmnOE$react, {useRef as $gmnOE$useRef, useMemo as $gmnOE$useMemo} from "react";
import {useTooltipTriggerState as $gmnOE$useTooltipTriggerState} from "react-stately/useTooltipTriggerState";

/*
 * Copyright 2026 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing permissions and limitations under the License.
 */ 






function $7e17edf2fd1a9983$export$60a5cf113499de66(props) {
    var _props_delay, _props_closeDelay;
    let state = (0, $gmnOE$useTooltipTriggerState)({
        ...props,
        delay: (_props_delay = props.delay) !== null && _props_delay !== void 0 ? _props_delay : 600,
        closeDelay: (_props_closeDelay = props.closeDelay) !== null && _props_closeDelay !== void 0 ? _props_closeDelay : 200
    });
    let triggerRef = (0, $gmnOE$useRef)(null);
    let popoverRef = (0, $gmnOE$useRef)(null);
    let { triggerProps: triggerProps, popoverProps: popoverProps } = (0, $gmnOE$usePreviewTrigger)({
        ...props,
        triggerRef: triggerRef,
        popoverRef: popoverRef
    }, state);
    // The Popover and usePopover expect an OverlayTriggerState. Adapt the TooltipTriggerState (which
    // provides the warmup/cooldown delay behavior) to that interface.
    let overlayState = (0, $gmnOE$useMemo)(()=>({
            isOpen: state.isOpen,
            open: ()=>state.open(),
            close: ()=>state.close(),
            setOpen: (isOpen)=>isOpen ? state.open() : state.close(),
            toggle: ()=>state.isOpen ? state.close() : state.open(),
            point: null,
            setPoint: ()=>{}
        }), [
        state
    ]);
    return /*#__PURE__*/ (0, $gmnOE$react).createElement((0, $b7b7a92703138c9b$export$2881499e37b75b9a), {
        values: [
            [
                (0, $acf8e70c2f419f18$export$d2f961adcb0afbe),
                overlayState
            ],
            [
                (0, $03df2f2d3cffb62f$export$9b9a0cd73afb7ca4),
                {
                    trigger: 'PreviewTrigger',
                    triggerRef: triggerRef,
                    ref: popoverRef,
                    isNonModal: true,
                    // Skip enter/exit animations when swapping between previews during the warmup period.
                    shouldSkipAnimation: state.shouldSkipAnimation,
                    ...popoverProps
                }
            ]
        ]
    }, /*#__PURE__*/ (0, $gmnOE$react).createElement((0, $gmnOE$FocusableProvider), {
        ...triggerProps,
        ref: triggerRef
    }, props.children));
}


export {$7e17edf2fd1a9983$export$60a5cf113499de66 as PreviewTrigger};
//# sourceMappingURL=PreviewTrigger.js.map
