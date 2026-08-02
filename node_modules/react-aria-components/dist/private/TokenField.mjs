import {dom as $7230ffa83bc0c2cf$export$df3a06d6289f983e, Provider as $7230ffa83bc0c2cf$export$2881499e37b75b9a, useContextProps as $7230ffa83bc0c2cf$export$29f1550f4b0d4415, useRenderProps as $7230ffa83bc0c2cf$export$4d86445c2cf5e3, useSlot as $7230ffa83bc0c2cf$export$9d4c57ee4c6ffdd8, useSlottedContext as $7230ffa83bc0c2cf$export$fabf2dc03a41866e} from "./utils.mjs";
import {FieldInputContext as $4b38b5b75ecc6208$export$698f465ec27e93df} from "./Autocomplete.mjs";
import {LabelContext as $43a3b93638fe5db9$export$75b6ee27786ba447} from "./Label.mjs";
import {TextContext as $efe09c6d1c304b50$export$9afb8bc826b033ea} from "./Text.mjs";
import {createHideableComponent as $iRP3W$createHideableComponent} from "react-aria/private/collections/Hidden";
import {filterDOMProps as $iRP3W$filterDOMProps} from "react-aria/filterDOMProps";
import {useHover as $iRP3W$useHover} from "react-aria/useHover";
import {mergeProps as $iRP3W$mergeProps} from "react-aria/mergeProps";
import {mergeRefs as $iRP3W$mergeRefs} from "react-aria/mergeRefs";
import $iRP3W$react, {createContext as $iRP3W$createContext, forwardRef as $iRP3W$forwardRef, useContext as $iRP3W$useContext, useMemo as $iRP3W$useMemo, memo as $iRP3W$memo} from "react";
import {useTokenFieldState as $iRP3W$useTokenFieldState} from "react-stately/useTokenFieldState";
import {useFocusRing as $iRP3W$useFocusRing} from "react-aria/useFocusRing";
import {useObjectRef as $iRP3W$useObjectRef} from "react-aria/useObjectRef";
import {useTokenField as $iRP3W$useTokenField, useToken as $iRP3W$useToken} from "react-aria/useTokenField";

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













const $45dd9fe7ca53fba4$export$24532e5e37c56ff2 = /*#__PURE__*/ (0, $iRP3W$createContext)(null);
const $45dd9fe7ca53fba4$var$TokenInputContext = /*#__PURE__*/ (0, $iRP3W$createContext)(null);
const $45dd9fe7ca53fba4$export$e4550c392cbe69d7 = /*#__PURE__*/ (0, $iRP3W$createHideableComponent)(function TokenField(props, ref) {
    [props, ref] = (0, $7230ffa83bc0c2cf$export$29f1550f4b0d4415)(props, ref, $45dd9fe7ca53fba4$export$24532e5e37c56ff2);
    let [labelRef, label] = (0, $7230ffa83bc0c2cf$export$9d4c57ee4c6ffdd8)(!props['aria-label'] && !props['aria-labelledby']);
    let fieldCtx = (0, $7230ffa83bc0c2cf$export$fabf2dc03a41866e)((0, $4b38b5b75ecc6208$export$698f465ec27e93df), props.slot);
    let { value: _autocompleteValue, onChange: onAutocompleteChange, ref: autocompleteRef, ...autocompleteProps } = fieldCtx ?? {};
    let inputRef = (0, $iRP3W$useObjectRef)(autocompleteRef);
    let isDisabled = props.isDisabled || false;
    let isReadOnly = props.isReadOnly || false;
    let state = (0, $iRP3W$useTokenFieldState)({
        ...props,
        onChange: (value)=>{
            props.onChange?.(value);
            onAutocompleteChange?.(value.toString());
        }
    });
    let { tokenFieldProps: tokenFieldProps, labelProps: labelProps, descriptionProps: descriptionProps } = (0, $iRP3W$useTokenField)({
        ...props,
        label: // @ts-ignore - not a public prop, used to determine if slot is present
        label,
        role: props.role || autocompleteProps['role'] || 'textbox'
    }, state, inputRef);
    let renderProps = (0, $7230ffa83bc0c2cf$export$4d86445c2cf5e3)({
        ...props,
        values: {
            isDisabled: isDisabled,
            isReadOnly: isReadOnly
        },
        defaultClassName: 'react-aria-TokenField'
    });
    let DOMProps = (0, $iRP3W$filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, $iRP3W$react).createElement((0, $7230ffa83bc0c2cf$export$df3a06d6289f983e).div, {
        ...DOMProps,
        ...renderProps,
        ref: ref,
        slot: props.slot || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined
    }, /*#__PURE__*/ (0, $iRP3W$react).createElement((0, $7230ffa83bc0c2cf$export$2881499e37b75b9a), {
        values: [
            [
                (0, $43a3b93638fe5db9$export$75b6ee27786ba447),
                {
                    ...labelProps,
                    elementType: 'span',
                    ref: labelRef
                }
            ],
            [
                (0, $efe09c6d1c304b50$export$9afb8bc826b033ea),
                {
                    slots: {
                        description: descriptionProps
                    }
                }
            ],
            [
                $45dd9fe7ca53fba4$var$TokenInputContext,
                {
                    tokenFieldProps: tokenFieldProps,
                    state: state,
                    isDisabled: isDisabled,
                    isReadOnly: isReadOnly,
                    autocompleteProps: autocompleteProps,
                    ref: inputRef
                }
            ]
        ]
    }, renderProps.children));
});
const $45dd9fe7ca53fba4$export$4ddf413e820cfcc4 = /*#__PURE__*/ (0, $iRP3W$forwardRef)(function TokenInput(props, forwardedRef) {
    let { tokenFieldProps: tokenFieldProps, state: state, isDisabled: isDisabled = false, isReadOnly: isReadOnly = false, autocompleteProps: autocompleteProps, ref: contextRef } = (0, $iRP3W$useContext)($45dd9fe7ca53fba4$var$TokenInputContext);
    let ref = (0, $iRP3W$useMemo)(()=>(0, $iRP3W$mergeRefs)(contextRef, forwardedRef), [
        contextRef,
        forwardedRef
    ]);
    let { children: children, ...domProps } = props;
    let { isHovered: isHovered, hoverProps: hoverProps } = (0, $iRP3W$useHover)(domProps);
    let { isFocused: isFocused, isFocusVisible: isFocusVisible, focusProps: focusProps } = (0, $iRP3W$useFocusRing)();
    let renderProps = (0, $7230ffa83bc0c2cf$export$4d86445c2cf5e3)({
        ...domProps,
        defaultClassName: 'react-aria-TokenInput',
        values: {
            isHovered: isHovered,
            isFocused: isFocused,
            isFocusVisible: isFocusVisible,
            isDisabled: isDisabled,
            isReadOnly: isReadOnly
        }
    });
    let DOMProps = (0, $iRP3W$filterDOMProps)(domProps, {
        global: true
    });
    return /*#__PURE__*/ (0, $iRP3W$react).createElement((0, $7230ffa83bc0c2cf$export$df3a06d6289f983e).div, {
        ...(0, $iRP3W$mergeProps)(DOMProps, renderProps, focusProps, hoverProps, tokenFieldProps, autocompleteProps),
        ref: ref,
        "data-focused": isFocused || undefined,
        "data-focus-visible": isFocusVisible || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined,
        style: {
            ...renderProps.style,
            ...tokenFieldProps?.style
        }
    }, /*#__PURE__*/ (0, $iRP3W$react).createElement($45dd9fe7ca53fba4$var$CompositionRenderBlocker, {
        isComposing: state.isComposing
    }, state.value.segments.map((v, i)=>{
        switch(v.type){
            case 'token':
                {
                    let token = children(v);
                    return(// Wrap tokens in zero-width spaces so the cursor is placed correctly.
                    /*#__PURE__*/ (0, $iRP3W$react).createElement("span", {
                        key: i
                    }, '\u200b', token, '\u200b'));
                }
            case 'text':
                return v.text;
        }
    }), state.value.segments.at(-1)?.text.endsWith('\n') && /*#__PURE__*/ (0, $iRP3W$react).createElement("br", null)));
});
const $45dd9fe7ca53fba4$export$50792b0e93539fde = /*#__PURE__*/ (0, $iRP3W$forwardRef)(function Token(props, ref) {
    let { isDisabled: isDisabled } = (0, $iRP3W$useContext)($45dd9fe7ca53fba4$var$TokenInputContext);
    let objectRef = (0, $iRP3W$useObjectRef)(ref);
    let { tokenProps: tokenProps, isSelected: isSelected } = (0, $iRP3W$useToken)(props, {}, objectRef);
    let renderProps = (0, $7230ffa83bc0c2cf$export$4d86445c2cf5e3)({
        ...props,
        defaultClassName: 'react-aria-Token',
        values: {
            isSelected: isSelected,
            isDisabled: isDisabled
        }
    });
    let DOMProps = (0, $iRP3W$filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, $iRP3W$react).createElement((0, $7230ffa83bc0c2cf$export$df3a06d6289f983e).span, {
        ref: objectRef,
        ...(0, $iRP3W$mergeProps)(DOMProps, renderProps, tokenProps),
        "data-selected": isSelected || undefined,
        "data-disabled": isDisabled || undefined,
        style: {
            ...renderProps.style,
            ...tokenProps.style
        }
    }, renderProps.children);
});
// Prevents React from re-rendering during composition events.
const $45dd9fe7ca53fba4$var$CompositionRenderBlocker = /*#__PURE__*/ (0, $iRP3W$memo)(({ children: children })=>children, (prevProps, nextProps)=>nextProps.isComposing ? true : prevProps.children === nextProps.children);


export {$45dd9fe7ca53fba4$export$24532e5e37c56ff2 as TokenFieldContext, $45dd9fe7ca53fba4$export$e4550c392cbe69d7 as TokenField, $45dd9fe7ca53fba4$export$4ddf413e820cfcc4 as TokenInput, $45dd9fe7ca53fba4$export$50792b0e93539fde as Token};
//# sourceMappingURL=TokenField.mjs.map
