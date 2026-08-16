import {dom as $b7b7a92703138c9b$export$df3a06d6289f983e, Provider as $b7b7a92703138c9b$export$2881499e37b75b9a, useContextProps as $b7b7a92703138c9b$export$29f1550f4b0d4415, useRenderProps as $b7b7a92703138c9b$export$4d86445c2cf5e3, useSlot as $b7b7a92703138c9b$export$9d4c57ee4c6ffdd8, useSlottedContext as $b7b7a92703138c9b$export$fabf2dc03a41866e} from "./utils.js";
import {FieldInputContext as $8f09b710ef85b337$export$698f465ec27e93df} from "./Autocomplete.js";
import {LabelContext as $3e4839e5b30e7b17$export$75b6ee27786ba447} from "./Label.js";
import {TextContext as $20d769b1e2b13352$export$9afb8bc826b033ea} from "./Text.js";
import {createHideableComponent as $lOx05$createHideableComponent} from "react-aria/private/collections/Hidden";
import {filterDOMProps as $lOx05$filterDOMProps} from "react-aria/filterDOMProps";
import {useHover as $lOx05$useHover} from "react-aria/useHover";
import {mergeProps as $lOx05$mergeProps} from "react-aria/mergeProps";
import {mergeRefs as $lOx05$mergeRefs} from "react-aria/mergeRefs";
import $lOx05$react, {createContext as $lOx05$createContext, forwardRef as $lOx05$forwardRef, useContext as $lOx05$useContext, useMemo as $lOx05$useMemo, memo as $lOx05$memo} from "react";
import {useTokenFieldState as $lOx05$useTokenFieldState} from "react-stately/useTokenFieldState";
import {useFocusRing as $lOx05$useFocusRing} from "react-aria/useFocusRing";
import {useObjectRef as $lOx05$useObjectRef} from "react-aria/useObjectRef";
import {useTokenField as $lOx05$useTokenField, useToken as $lOx05$useToken} from "react-aria/useTokenField";

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













const $0557236f0f66b699$export$24532e5e37c56ff2 = /*#__PURE__*/ (0, $lOx05$createContext)(null);
const $0557236f0f66b699$var$TokenInputContext = /*#__PURE__*/ (0, $lOx05$createContext)(null);
const $0557236f0f66b699$export$e4550c392cbe69d7 = /*#__PURE__*/ (0, $lOx05$createHideableComponent)(function TokenField(props, ref) {
    [props, ref] = (0, $b7b7a92703138c9b$export$29f1550f4b0d4415)(props, ref, $0557236f0f66b699$export$24532e5e37c56ff2);
    let [labelRef, label] = (0, $b7b7a92703138c9b$export$9d4c57ee4c6ffdd8)(!props['aria-label'] && !props['aria-labelledby']);
    let fieldCtx = (0, $b7b7a92703138c9b$export$fabf2dc03a41866e)((0, $8f09b710ef85b337$export$698f465ec27e93df), props.slot);
    let { value: _autocompleteValue, onChange: onAutocompleteChange, ref: autocompleteRef, ...autocompleteProps } = fieldCtx !== null && fieldCtx !== void 0 ? fieldCtx : {};
    let inputRef = (0, $lOx05$useObjectRef)(autocompleteRef);
    let isDisabled = props.isDisabled || false;
    let isReadOnly = props.isReadOnly || false;
    let state = (0, $lOx05$useTokenFieldState)({
        ...props,
        onChange: (value)=>{
            var _props_onChange;
            (_props_onChange = props.onChange) === null || _props_onChange === void 0 ? void 0 : _props_onChange.call(props, value);
            onAutocompleteChange === null || onAutocompleteChange === void 0 ? void 0 : onAutocompleteChange(value.toString());
        }
    });
    let { tokenFieldProps: tokenFieldProps, labelProps: labelProps, descriptionProps: descriptionProps } = (0, $lOx05$useTokenField)({
        ...props,
        label: // @ts-ignore - not a public prop, used to determine if slot is present
        label,
        role: props.role || autocompleteProps['role'] || 'textbox'
    }, state, inputRef);
    let renderProps = (0, $b7b7a92703138c9b$export$4d86445c2cf5e3)({
        ...props,
        values: {
            isDisabled: isDisabled,
            isReadOnly: isReadOnly
        },
        defaultClassName: 'react-aria-TokenField'
    });
    let DOMProps = (0, $lOx05$filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, $lOx05$react).createElement((0, $b7b7a92703138c9b$export$df3a06d6289f983e).div, {
        ...DOMProps,
        ...renderProps,
        ref: ref,
        slot: props.slot || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined
    }, /*#__PURE__*/ (0, $lOx05$react).createElement((0, $b7b7a92703138c9b$export$2881499e37b75b9a), {
        values: [
            [
                (0, $3e4839e5b30e7b17$export$75b6ee27786ba447),
                {
                    ...labelProps,
                    elementType: 'span',
                    ref: labelRef
                }
            ],
            [
                (0, $20d769b1e2b13352$export$9afb8bc826b033ea),
                {
                    slots: {
                        description: descriptionProps
                    }
                }
            ],
            [
                $0557236f0f66b699$var$TokenInputContext,
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
const $0557236f0f66b699$export$4ddf413e820cfcc4 = /*#__PURE__*/ (0, $lOx05$forwardRef)(function TokenInput(props, forwardedRef) {
    var _state_value_segments_at;
    let { tokenFieldProps: tokenFieldProps, state: state, isDisabled: isDisabled = false, isReadOnly: isReadOnly = false, autocompleteProps: autocompleteProps, ref: contextRef } = (0, $lOx05$useContext)($0557236f0f66b699$var$TokenInputContext);
    let ref = (0, $lOx05$useMemo)(()=>(0, $lOx05$mergeRefs)(contextRef, forwardedRef), [
        contextRef,
        forwardedRef
    ]);
    let { children: children, ...domProps } = props;
    let { isHovered: isHovered, hoverProps: hoverProps } = (0, $lOx05$useHover)(domProps);
    let { isFocused: isFocused, isFocusVisible: isFocusVisible, focusProps: focusProps } = (0, $lOx05$useFocusRing)();
    let renderProps = (0, $b7b7a92703138c9b$export$4d86445c2cf5e3)({
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
    let DOMProps = (0, $lOx05$filterDOMProps)(domProps, {
        global: true
    });
    return /*#__PURE__*/ (0, $lOx05$react).createElement((0, $b7b7a92703138c9b$export$df3a06d6289f983e).div, {
        ...(0, $lOx05$mergeProps)(DOMProps, renderProps, focusProps, hoverProps, tokenFieldProps, autocompleteProps),
        ref: ref,
        "data-focused": isFocused || undefined,
        "data-focus-visible": isFocusVisible || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined,
        style: {
            ...renderProps.style,
            ...tokenFieldProps === null || tokenFieldProps === void 0 ? void 0 : tokenFieldProps.style
        }
    }, /*#__PURE__*/ (0, $lOx05$react).createElement($0557236f0f66b699$var$CompositionRenderBlocker, {
        isComposing: state.isComposing
    }, state.value.segments.map((v, i)=>{
        switch(v.type){
            case 'token':
                {
                    let token = children(v);
                    return(// Wrap tokens in zero-width spaces so the cursor is placed correctly.
                    /*#__PURE__*/ (0, $lOx05$react).createElement("span", {
                        key: i
                    }, '\u200b', token, '\u200b'));
                }
            case 'text':
                return v.text;
        }
    }), ((_state_value_segments_at = state.value.segments.at(-1)) === null || _state_value_segments_at === void 0 ? void 0 : _state_value_segments_at.text.endsWith('\n')) && /*#__PURE__*/ (0, $lOx05$react).createElement("br", null)));
});
const $0557236f0f66b699$export$50792b0e93539fde = /*#__PURE__*/ (0, $lOx05$forwardRef)(function Token(props, ref) {
    let { isDisabled: isDisabled } = (0, $lOx05$useContext)($0557236f0f66b699$var$TokenInputContext);
    let objectRef = (0, $lOx05$useObjectRef)(ref);
    let { tokenProps: tokenProps, isSelected: isSelected } = (0, $lOx05$useToken)(props, {}, objectRef);
    let renderProps = (0, $b7b7a92703138c9b$export$4d86445c2cf5e3)({
        ...props,
        defaultClassName: 'react-aria-Token',
        values: {
            isSelected: isSelected,
            isDisabled: isDisabled
        }
    });
    let DOMProps = (0, $lOx05$filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, $lOx05$react).createElement((0, $b7b7a92703138c9b$export$df3a06d6289f983e).span, {
        ref: objectRef,
        ...(0, $lOx05$mergeProps)(DOMProps, renderProps, tokenProps),
        "data-selected": isSelected || undefined,
        "data-disabled": isDisabled || undefined,
        style: {
            ...renderProps.style,
            ...tokenProps.style
        }
    }, renderProps.children);
});
// Prevents React from re-rendering during composition events.
const $0557236f0f66b699$var$CompositionRenderBlocker = /*#__PURE__*/ (0, $lOx05$memo)(({ children: children })=>children, (prevProps, nextProps)=>nextProps.isComposing ? true : prevProps.children === nextProps.children);


export {$0557236f0f66b699$export$24532e5e37c56ff2 as TokenFieldContext, $0557236f0f66b699$export$e4550c392cbe69d7 as TokenField, $0557236f0f66b699$export$4ddf413e820cfcc4 as TokenInput, $0557236f0f66b699$export$50792b0e93539fde as Token};
//# sourceMappingURL=TokenField.js.map
