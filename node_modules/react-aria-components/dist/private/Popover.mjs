import {dom as $7230ffa83bc0c2cf$export$df3a06d6289f983e, useContextProps as $7230ffa83bc0c2cf$export$29f1550f4b0d4415, useRenderProps as $7230ffa83bc0c2cf$export$4d86445c2cf5e3} from "./utils.mjs";
import {OverlayArrowContext as $4fcfe18fac72dabd$export$2de4954e8ae13b9f} from "./OverlayArrow.mjs";
import {OverlayTriggerStateContext as $f2ff30fde7b014be$export$d2f961adcb0afbe} from "./Dialog.mjs";
import {usePopover as $g4jvP$usePopover} from "react-aria/usePopover";
import {DismissButton as $g4jvP$DismissButton, Overlay as $g4jvP$Overlay} from "react-aria/Overlay";
import {filterDOMProps as $g4jvP$filterDOMProps} from "react-aria/filterDOMProps";
import {focusSafely as $g4jvP$focusSafely} from "react-aria/private/interactions/focusSafely";
import {getInteractionModality as $g4jvP$getInteractionModality} from "react-aria/private/interactions/useFocusVisible";
import {isFocusWithin as $g4jvP$isFocusWithin} from "react-aria/private/utils/shadowdom/DOMFunctions";
import {mergeProps as $g4jvP$mergeProps} from "react-aria/mergeProps";
import {useOverlayTriggerState as $g4jvP$useOverlayTriggerState} from "react-stately/useOverlayTriggerState";
import $g4jvP$react, {createContext as $g4jvP$createContext, forwardRef as $g4jvP$forwardRef, useContext as $g4jvP$useContext, useRef as $g4jvP$useRef, useState as $g4jvP$useState, useEffect as $g4jvP$useEffect, useMemo as $g4jvP$useMemo, useCallback as $g4jvP$useCallback} from "react";
import {useExitAnimation as $g4jvP$useExitAnimation, useEnterAnimation as $g4jvP$useEnterAnimation} from "react-aria/private/utils/animation";
import {useIsHidden as $g4jvP$useIsHidden} from "react-aria/private/collections/Hidden";
import {useLayoutEffect as $g4jvP$useLayoutEffect} from "react-aria/private/utils/useLayoutEffect";
import {useLocale as $g4jvP$useLocale} from "react-aria/I18nProvider";
import {useResizeObserver as $g4jvP$useResizeObserver} from "react-aria/private/utils/useResizeObserver";

/*
 * Copyright 2022 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing permissions and limitations under the License.
 */ 
















const $542a13ca2fa5b484$export$9b9a0cd73afb7ca4 = /*#__PURE__*/ (0, $g4jvP$createContext)(null);
// Stores a ref for the portal container for a group of popovers (e.g. submenus).
const $542a13ca2fa5b484$var$PopoverGroupContext = /*#__PURE__*/ (0, $g4jvP$createContext)(null);
const $542a13ca2fa5b484$export$5b6b19405a83ff9d = /*#__PURE__*/ (0, $g4jvP$forwardRef)(function Popover(props, ref) {
    [props, ref] = (0, $7230ffa83bc0c2cf$export$29f1550f4b0d4415)(props, ref, $542a13ca2fa5b484$export$9b9a0cd73afb7ca4);
    let contextState = (0, $g4jvP$useContext)((0, $f2ff30fde7b014be$export$d2f961adcb0afbe));
    let localState = (0, $g4jvP$useOverlayTriggerState)(props);
    let state = props.isOpen != null || props.defaultOpen != null || !contextState ? localState : contextState;
    // Skip the automatic exit animation when closing instantly (e.g. swapping between previews
    // during warmup). An explicitly provided isExiting prop still takes precedence.
    let exitAnimation = (0, $g4jvP$useExitAnimation)(ref, state.isOpen);
    let isExiting = props.isExiting || !props.shouldSkipAnimation && exitAnimation || false;
    let isHidden = (0, $g4jvP$useIsHidden)();
    let { direction: direction } = (0, $g4jvP$useLocale)();
    // If we are in a hidden tree, we still need to preserve our children.
    if (isHidden) {
        let children = props.children;
        if (typeof children === 'function') children = children({
            trigger: props.trigger || null,
            placement: 'bottom',
            isEntering: false,
            isExiting: false,
            defaultChildren: null
        });
        return /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $g4jvP$react).Fragment, null, children);
    }
    if (state && !state.isOpen && !isExiting) return null;
    return /*#__PURE__*/ (0, $g4jvP$react).createElement($542a13ca2fa5b484$var$PopoverInner, {
        ...props,
        triggerRef: props.triggerRef,
        state: state,
        popoverRef: ref,
        isExiting: isExiting,
        dir: direction
    });
});
function $542a13ca2fa5b484$var$PopoverInner({ state: state, isExiting: isExiting, UNSTABLE_portalContainer: UNSTABLE_portalContainer, clearContexts: clearContexts, ...props }) {
    // Calculate the arrow size internally (and remove props.arrowSize from PopoverProps)
    // Referenced from: packages/@react-spectrum/tooltip/src/TooltipTrigger.tsx
    let arrowRef = (0, $g4jvP$useRef)(null);
    let containerRef = (0, $g4jvP$useRef)(null);
    let groupCtx = (0, $g4jvP$useContext)($542a13ca2fa5b484$var$PopoverGroupContext);
    let isSubPopover = groupCtx && props.trigger === 'SubmenuTrigger';
    let { popoverProps: popoverProps, underlayProps: underlayProps, arrowProps: arrowProps, placement: placement, triggerAnchorPoint: triggerAnchorPoint } = (0, $g4jvP$usePopover)({
        ...props,
        offset: props.offset ?? 8,
        arrowRef: arrowRef,
        // If this is a submenu/subdialog, use the root popover's container
        // to detect outside interaction and add aria-hidden.
        groupRef: isSubPopover ? groupCtx : containerRef
    }, state);
    let ref = props.popoverRef;
    // Skip the automatic entry animation when opening instantly (e.g. swapping between previews
    // during warmup). An explicitly provided isEntering prop still takes precedence.
    let enterAnimation = (0, $g4jvP$useEnterAnimation)(ref, !!placement);
    // oxlint-disable-next-line react/react-compiler
    let isEntering = props.isEntering || !props.shouldSkipAnimation && enterAnimation || false;
    // oxlint-disable-next-line react/react-compiler
    let renderProps = (0, $7230ffa83bc0c2cf$export$4d86445c2cf5e3)({
        // oxlint-disable-next-line react/react-compiler
        ...props,
        defaultClassName: 'react-aria-Popover',
        // oxlint-disable-next-line react/react-compiler
        values: {
            // oxlint-disable-next-line react/react-compiler
            trigger: props.trigger || null,
            placement: placement,
            isEntering: // oxlint-disable-next-line react/react-compiler
            isEntering,
            isExiting: isExiting
        }
    });
    // Automatically render Popover with role=dialog except when isNonModal is true,
    // or a dialog is already nested inside the popover.
    let shouldBeDialog = // oxlint-disable-next-line react/react-compiler
    !props.isNonModal || props.trigger === 'SubmenuTrigger' || props.trigger === 'PreviewTrigger';
    // oxlint-disable-next-line react/react-compiler
    let [isDialog, setDialog] = (0, $g4jvP$useState)(props.trigger === 'PreviewTrigger');
    (0, $g4jvP$useLayoutEffect)(()=>{
        if (ref.current) setDialog(shouldBeDialog && !ref.current.querySelector('[role=dialog]'));
    }, [
        ref,
        shouldBeDialog
    ]);
    // Focus the popover itself on mount, unless a child element is already focused.
    // Skip this for submenus since hovering a submenutrigger should keep focus on the trigger
    // oxlint-disable react/react-compiler
    (0, $g4jvP$useEffect)(()=>{
        if (isDialog && props.trigger !== 'PreviewTrigger' && (props.trigger !== 'SubmenuTrigger' || (0, $g4jvP$getInteractionModality)() !== 'pointer') && ref.current && !(0, $g4jvP$isFocusWithin)(ref.current)) (0, $g4jvP$focusSafely)(ref.current);
    }, [
        isDialog,
        ref,
        props.trigger
    ]);
    // oxlint-enable react/react-compiler
    let children = (0, $g4jvP$useMemo)(()=>{
        let children = renderProps.children;
        if (clearContexts) for (let Context of clearContexts)children = /*#__PURE__*/ (0, $g4jvP$react).createElement(Context.Provider, {
            value: null
        }, children);
        return children;
    }, [
        renderProps.children,
        clearContexts
    ]);
    let [triggerWidth, setTriggerWidth] = (0, $g4jvP$useState)(null);
    // oxlint-disable-next-line react/react-compiler
    let onResize = (0, $g4jvP$useCallback)(()=>{
        if (props.triggerRef.current) setTriggerWidth(props.triggerRef.current.getBoundingClientRect().width + 'px');
    }, [
        props.triggerRef
    ]);
    (0, $g4jvP$useLayoutEffect)(onResize, [
        onResize
    ]);
    // oxlint-disable-next-line react/react-compiler
    (0, $g4jvP$useResizeObserver)({
        // oxlint-disable-next-line react/react-compiler
        ref: renderProps.style?.['--trigger-width'] ? undefined : props.triggerRef,
        onResize: onResize
    });
    let style = {
        ...popoverProps.style,
        '--trigger-anchor-point': triggerAnchorPoint ? `${triggerAnchorPoint.x}px ${triggerAnchorPoint.y}px` : undefined,
        ...renderProps.style,
        '--trigger-width': renderProps.style?.['--trigger-width'] || triggerWidth
    };
    // oxlint-disable react/react-compiler
    let overlay = /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $7230ffa83bc0c2cf$export$df3a06d6289f983e).div, {
        ...(0, $g4jvP$mergeProps)((0, $g4jvP$filterDOMProps)(props, {
            global: true
        }), popoverProps),
        ...renderProps,
        id: isDialog ? props.id : undefined,
        role: isDialog ? 'dialog' : undefined,
        tabIndex: isDialog ? -1 : undefined,
        "aria-label": props['aria-label'],
        "aria-labelledby": props['aria-labelledby'],
        ref: ref,
        slot: props.slot || undefined,
        style: style,
        dir: props.dir,
        "data-trigger": props.trigger,
        "data-placement": placement,
        "data-entering": isEntering || undefined,
        "data-exiting": isExiting || undefined
    }, !props.isNonModal && /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $g4jvP$DismissButton), {
        onDismiss: state.close
    }), /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $4fcfe18fac72dabd$export$2de4954e8ae13b9f).Provider, {
        value: {
            ...arrowProps,
            placement: placement,
            ref: arrowRef
        }
    }, children), /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $g4jvP$DismissButton), {
        onDismiss: state.close
    }));
    // oxlint-enable react/react-compiler
    // If this is a root popover, render an extra div to act as the portal container for submenus/subdialogs.
    if (!isSubPopover) // oxlint-disable react/react-compiler
    return /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $g4jvP$Overlay), {
        ...props,
        shouldContainFocus: isDialog && props.trigger !== 'PreviewTrigger',
        isExiting: isExiting,
        portalContainer: UNSTABLE_portalContainer
    }, !props.isNonModal && state.isOpen && /*#__PURE__*/ (0, $g4jvP$react).createElement("div", {
        "data-testid": "underlay",
        ...underlayProps,
        style: {
            position: 'fixed',
            inset: 0
        }
    }), /*#__PURE__*/ (0, $g4jvP$react).createElement("div", {
        ref: containerRef,
        style: {
            display: 'contents'
        }
    }, /*#__PURE__*/ (0, $g4jvP$react).createElement($542a13ca2fa5b484$var$PopoverGroupContext.Provider, {
        value: containerRef
    }, overlay)));
    // Submenus/subdialogs are mounted into the root popover's container.
    // oxlint-disable react/react-compiler
    return /*#__PURE__*/ (0, $g4jvP$react).createElement((0, $g4jvP$Overlay), {
        ...props,
        shouldContainFocus: isDialog && props.trigger !== 'PreviewTrigger',
        isExiting: isExiting,
        portalContainer: UNSTABLE_portalContainer ?? groupCtx?.current ?? undefined
    }, overlay);
// oxlint-enable react/react-compiler
}


export {$542a13ca2fa5b484$export$9b9a0cd73afb7ca4 as PopoverContext, $542a13ca2fa5b484$export$5b6b19405a83ff9d as Popover};
//# sourceMappingURL=Popover.mjs.map
