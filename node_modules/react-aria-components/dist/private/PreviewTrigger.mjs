import {OverlayTriggerStateContext as $f2ff30fde7b014be$export$d2f961adcb0afbe} from "./Dialog.mjs";
import {PopoverContext as $542a13ca2fa5b484$export$9b9a0cd73afb7ca4} from "./Popover.mjs";
import {Provider as $7230ffa83bc0c2cf$export$2881499e37b75b9a} from "./utils.mjs";
import {usePreviewTrigger as $evW8Q$usePreviewTrigger} from "react-aria/usePreviewTrigger";
import {FocusableProvider as $evW8Q$FocusableProvider} from "react-aria/private/interactions/useFocusable";
import $evW8Q$react, {useRef as $evW8Q$useRef, useMemo as $evW8Q$useMemo} from "react";
import {useTooltipTriggerState as $evW8Q$useTooltipTriggerState} from "react-stately/useTooltipTriggerState";

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






function $32e5dbc959387ef1$export$60a5cf113499de66(props) {
    let state = (0, $evW8Q$useTooltipTriggerState)({
        ...props,
        delay: props.delay ?? 600,
        closeDelay: props.closeDelay ?? 200
    });
    let triggerRef = (0, $evW8Q$useRef)(null);
    let popoverRef = (0, $evW8Q$useRef)(null);
    let { triggerProps: triggerProps, popoverProps: popoverProps } = (0, $evW8Q$usePreviewTrigger)({
        ...props,
        triggerRef: triggerRef,
        popoverRef: popoverRef
    }, state);
    // The Popover and usePopover expect an OverlayTriggerState. Adapt the TooltipTriggerState (which
    // provides the warmup/cooldown delay behavior) to that interface.
    let overlayState = (0, $evW8Q$useMemo)(()=>({
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
    return /*#__PURE__*/ (0, $evW8Q$react).createElement((0, $7230ffa83bc0c2cf$export$2881499e37b75b9a), {
        values: [
            [
                (0, $f2ff30fde7b014be$export$d2f961adcb0afbe),
                overlayState
            ],
            [
                (0, $542a13ca2fa5b484$export$9b9a0cd73afb7ca4),
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
    }, /*#__PURE__*/ (0, $evW8Q$react).createElement((0, $evW8Q$FocusableProvider), {
        ...triggerProps,
        ref: triggerRef
    }, props.children));
}


export {$32e5dbc959387ef1$export$60a5cf113499de66 as PreviewTrigger};
//# sourceMappingURL=PreviewTrigger.mjs.map
