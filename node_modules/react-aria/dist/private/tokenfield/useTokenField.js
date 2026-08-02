import {announce as $a53edfcc12185fd0$export$a9b970dcc4ae71a9} from "../live-announcer/LiveAnnouncer.js";
import {getActiveElement as $d8ac7ed472840322$export$cd4e5573fbe2b576} from "../utils/shadowdom/DOMFunctions.js";
import {getOwnerDocument as $cc3c3666b64debad$export$b204af158042fbac} from "../utils/domHelpers.js";
import {isMac as $d5a2be505488529f$export$9ac100e40613ea10} from "../utils/platform.js";
import {mergeProps as $64c36edd757dfa16$export$9d1611c77c2fe928} from "../utils/mergeProps.js";
import {setInteractionModality as $b50b1cc8a843ace7$export$8397ddfc504fdb9a} from "../interactions/useFocusVisible.js";
import {useEvent as $c3cab330536504ec$export$90fc3a17d93f704c} from "../utils/useEvent.js";
import {useField as $b5d79d79d9c34c91$export$294aa081a6c6f55d} from "../label/useField.js";
import {useFocusable as $088f27a386bc4a8f$export$4c014de7c8940b4c} from "../interactions/useFocusable.js";
import {useKeyboard as $bf74df7506f65576$export$8f71654801c2f7cd} from "../interactions/useKeyboard.js";
import {useLayoutEffect as $53fed047b798be36$export$e5c5a5f917a5871c} from "../utils/useLayoutEffect.js";
import {useLocale as $4defb058003b3e05$export$43bb16f9c6d9e3f7} from "../i18n/I18nProvider.js";
import {useMemo as $88wHG$useMemo, useRef as $88wHG$useRef, useCallback as $88wHG$useCallback} from "react";
import {TokenFieldValue as $88wHG$TokenFieldValue} from "react-stately/useTokenFieldState";

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













const $6b58de437c931efd$var$CLIPBOARD_MIME_TYPE = 'application/vnd.react-aria.tokens+json';
function $6b58de437c931efd$export$adf3c8de3c7afc8b(props, state, ref) {
    let { role: role = 'textbox', allowsNewlines: multiline = false, isReadOnly: isReadOnly = false, isDisabled: isDisabled = false, 'aria-details': ariaDetails } = props;
    let { value: value } = state;
    let { locale: locale } = (0, $4defb058003b3e05$export$43bb16f9c6d9e3f7)();
    let graphemeSegmenter = (0, $88wHG$useMemo)(()=>new Intl.Segmenter(locale, {
            granularity: 'grapheme'
        }), [
        locale
    ]);
    let wordSegmenter = (0, $88wHG$useMemo)(()=>new Intl.Segmenter(locale, {
            granularity: 'word'
        }), [
        locale
    ]);
    let dropPosition = (0, $88wHG$useRef)(null);
    let transferredData = (0, $88wHG$useRef)(null);
    let nextValue = (0, $88wHG$useRef)(null);
    let apply = (fn)=>{
        state.setValue((value)=>{
            let newValue = fn(value);
            nextValue.current = newValue;
            return newValue;
        });
    };
    // Composition events are not cancelable. The browser will mutate the DOM, making it out of sync with React.
    // To account for this, we prevent React from re-rendering during composition, and track DOM mutations performed
    // by the browser. When composition ends, we revert the DOM to its original state, and re-render with React.
    // Mutating the DOM in any way during composition breaks the IME, causing composition to end unexpectedly.
    // During composition, we still emit updates via onChange to ensure that things like autocomplete work,
    // but we don't actually re-render to the DOM unless the value changes from what we expect (e.g. inserting a completion).
    let mutationTracker = $6b58de437c931efd$var$useMutationTracker(ref);
    let startComposition = (0, $88wHG$useCallback)(()=>{
        mutationTracker.start();
        state.setComposing(true);
    }, [
        state,
        mutationTracker
    ]);
    let stopComposition = (0, $88wHG$useCallback)(()=>{
        mutationTracker.stop();
        state.setComposing(false);
    }, [
        state,
        mutationTracker
    ]);
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'compositionstart', ()=>{
        var _window_getSelection;
        startComposition();
        let range = (_window_getSelection = window.getSelection()) === null || _window_getSelection === void 0 ? void 0 : _window_getSelection.getRangeAt(0);
        if (range) {
            let [start, end] = $6b58de437c931efd$var$rangeToPositions(ref.current, range);
            // Normalize the range to ensure it is not inside a token, otherwise the browser
            // will attempt to insert the composed text into the token instead of replacing it.
            let r = $6b58de437c931efd$var$createDOMRange(ref.current, start, end);
            if (r.startContainer !== range.startContainer || r.startOffset !== range.startOffset) range.setStart(r.startContainer, r.startOffset);
            if (r.endContainer !== range.endContainer || r.endOffset !== range.endOffset) range.setEnd(r.endContainer, r.endOffset);
        }
    });
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'compositionend', stopComposition);
    // If a prop update occurs during composition that doesn't match the expected value,
    // end composition and re-render the controlled value.
    (0, $53fed047b798be36$export$e5c5a5f917a5871c)(()=>{
        if (state.isComposing && value !== nextValue.current) stopComposition();
        nextValue.current = value;
    });
    let caretPosition = (0, $88wHG$useRef)(null);
    (0, $53fed047b798be36$export$e5c5a5f917a5871c)(()=>{
        if (ref.current && value.caretPosition && !state.isComposing && value.caretPosition !== caretPosition.current) {
            // Only move the caret when the field is already focused.
            if (ref.current === (0, $d8ac7ed472840322$export$cd4e5573fbe2b576)((0, $cc3c3666b64debad$export$b204af158042fbac)(ref.current))) $6b58de437c931efd$var$setCursor(ref.current, value.caretPosition);
            caretPosition.current = value.caretPosition;
        }
    });
    // Handle text editing commands and prevent browser default behavior.
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'beforeinput', (e)=>{
        // Android sometimes doesn't fire a compositionend event before a regular input event.
        if (state.isComposing && !e.isComposing) stopComposition();
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0) return;
        let range = selection.getRangeAt(0);
        let [start, end] = $6b58de437c931efd$var$rangeToPositions(ref.current, range);
        // https://www.w3.org/TR/input-events-2/#interface-InputEvent-Attributes
        switch(e.inputType){
            case 'insertText':
            case 'insertReplacementText':
            case 'insertCompositionText':
            case 'insertFromComposition':
            case 'insertFromPaste':
            case 'insertFromYank':
            case 'insertFromDrop':
                {
                    var _e_data;
                    let data = [
                        {
                            type: 'text',
                            text: (_e_data = e.data) !== null && _e_data !== void 0 ? _e_data : ''
                        }
                    ];
                    if (transferredData.current) {
                        data = transferredData.current;
                        transferredData.current = null;
                    } else if (e.dataTransfer) {
                        let parsed = e.dataTransfer.types.includes($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE) ? $6b58de437c931efd$var$parseSegments(e.dataTransfer.getData($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE)) : null;
                        if (parsed) data = parsed;
                        else if (e.dataTransfer.types.includes('text/plain')) data[0].text = e.dataTransfer.getData('text/plain');
                    }
                    if (e.inputType === 'insertFromDrop' && dropPosition.current) {
                        start = end = dropPosition.current;
                        dropPosition.current = null;
                    }
                    if (!multiline) for (let segment of data)segment.text = segment.text.replace(/[\r\n]+/g, ' ');
                    apply((tokens)=>tokens.replaceRangeWithSegments(start, end, data, // Don't coalesce paste/drop events with other edits.
                        e.inputType === 'insertText' || e.inputType === 'insertCompositionText' || e.inputType === 'insertFromComposition'));
                    break;
                }
            case 'insertParagraph':
                if (props.onSubmit) {
                    props.onSubmit();
                    break;
                }
            case 'insertLineBreak':
                if (multiline) apply((tokens)=>tokens.replaceRange(start, end, '\n'));
                break;
            case 'deleteContentBackward':
            case 'deleteContentForward':
            case 'deleteWordBackward':
            case 'deleteWordForward':
            case 'deleteHardLineForward':
            case 'deleteHardLineBackward':
            case 'deleteSoftLineForward':
            case 'deleteSoftLineBackward':
            case 'deleteContent':
            case 'deleteByCut':
            case 'deleteCompositionText':
                if (!range.collapsed) {
                    apply((tokens)=>tokens.replaceRange(start, end, ''));
                    break;
                }
                switch(e.inputType){
                    case 'deleteContentBackward':
                        apply((tokens)=>tokens.delete(start, graphemeSegmenter, (0, $88wHG$TokenFieldValue).Direction.Backward));
                        break;
                    case 'deleteContentForward':
                        apply((tokens)=>tokens.delete(start, graphemeSegmenter, (0, $88wHG$TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteWordBackward':
                        apply((tokens)=>tokens.delete(start, wordSegmenter, (0, $88wHG$TokenFieldValue).Direction.Backward));
                        break;
                    case 'deleteWordForward':
                        apply((tokens)=>tokens.delete(start, wordSegmenter, (0, $88wHG$TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteHardLineForward':
                    case 'deleteSoftLineForward':
                        apply((tokens)=>tokens.deleteLine(start, (0, $88wHG$TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteHardLineBackward':
                    case 'deleteSoftLineBackward':
                        apply((tokens)=>tokens.deleteLine(start, (0, $88wHG$TokenFieldValue).Direction.Backward));
                        break;
                }
                break;
            case 'deleteByDrag':
                apply((tokens)=>{
                    let endOffset = start.index === end.index ? end.offset : tokens.segments[start.index].text.length;
                    let change = tokens.replaceRange(start, end, '');
                    if (dropPosition.current && dropPosition.current.index === start.index && dropPosition.current.offset >= start.offset) dropPosition.current.offset -= endOffset - start.offset;
                    return change;
                });
                break;
        }
        e.preventDefault();
    });
    let writeClipboardData = (e)=>{
        if ('clipboardData' in e) e.preventDefault();
        let selection = $6b58de437c931efd$export$ca798a7e6e94638c(ref.current);
        if (!selection) return;
        let [start, end] = selection;
        let slice = value.slice(start, end);
        let dataTransfer = 'clipboardData' in e ? e.clipboardData : e.dataTransfer;
        dataTransfer === null || dataTransfer === void 0 ? void 0 : dataTransfer.setData($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE, JSON.stringify(slice.segments));
        dataTransfer === null || dataTransfer === void 0 ? void 0 : dataTransfer.setData('text/plain', slice.toString());
        if (e.type === 'cut') apply((tokens)=>tokens.replaceRange(start, end, '', false));
    };
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'copy', writeClipboardData);
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'cut', writeClipboardData);
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'dragstart', writeClipboardData);
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'paste', (e)=>{
        // Safari doesn't pass the custom clipboard data type to beforeinput dataTransfer so we handle it here.
        if (e.clipboardData && e.clipboardData.types.includes($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE)) transferredData.current = $6b58de437c931efd$var$parseSegments(e.clipboardData.getData($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE));
    });
    // Store the cursor position on drop so we know where to insert when the insertFromDrop event occurs.
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'drop', (e)=>{
        if (typeof document.caretPositionFromPoint === 'function') {
            let pos = document.caretPositionFromPoint(e.clientX, e.clientY);
            if (pos) dropPosition.current = $6b58de437c931efd$var$getPosition(ref.current, pos.offsetNode, pos.offset);
        } else if (typeof document.caretRangeFromPoint === 'function') {
            let range = document.caretRangeFromPoint(e.clientX, e.clientY);
            if (range) dropPosition.current = $6b58de437c931efd$var$getPosition(ref.current, range.startContainer, range.startOffset);
        }
        if (e.dataTransfer && e.dataTransfer.types.includes($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE)) transferredData.current = $6b58de437c931efd$var$parseSegments(e.dataTransfer.getData($6b58de437c931efd$var$CLIPBOARD_MIME_TYPE));
    });
    $6b58de437c931efd$var$useSelectionChange(ref, ()=>{
        var _window_getSelection;
        if (state.isComposing) return;
        value.endCoalescing();
        // When the cursor moves next to a token, announce it.
        // Otherwise the screen reader will only announce the first/last character.
        if ((_window_getSelection = window.getSelection()) === null || _window_getSelection === void 0 ? void 0 : _window_getSelection.isCollapsed) {
            let [start, end] = $6b58de437c931efd$export$ca798a7e6e94638c(ref.current);
            if (start.offset === 0) {
                let segment = value.segments[start.index];
                if ((segment === null || segment === void 0 ? void 0 : segment.type) !== 'token') segment = value.segments[start.index - 1];
                if ((segment === null || segment === void 0 ? void 0 : segment.type) === 'token') (0, $a53edfcc12185fd0$export$a9b970dcc4ae71a9)(segment.text, 'assertive');
                // Update the caret position in the value.
                state.setValue((value)=>value.withCaretPosition(end));
            }
        }
    });
    // Override the default triple click behavior to ensure that tokens get selected.
    // Some browsers only select the text between tokens instead of the entire line.
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)(ref, 'mousedown', (e)=>{
        if (e.detail === 3) {
            let selection = $6b58de437c931efd$export$ca798a7e6e94638c(ref.current);
            if (!selection) return;
            let start = value.findLineBoundary(selection[0], (0, $88wHG$TokenFieldValue).Direction.Backward);
            let end = value.findLineBoundary(selection[1], (0, $88wHG$TokenFieldValue).Direction.Forward);
            if (start && end) {
                e.preventDefault();
                $6b58de437c931efd$export$9cafaa09e78b6e4b(ref.current, start, end, true);
            }
        }
    });
    let moveSelection = (direction, granularity, extend = false)=>{
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0 || !selection.focusNode || !selection.anchorNode) return false;
        // Pressing an arrow with a non-empty selection collapses it to the corresponding edge.
        // The browser handles this natively.
        if (!extend && !selection.isCollapsed) return false;
        // Move the caret using the browser's native caret movement (Selection.modify) so that
        // bidirectional text is handled correctly. Repeat until the position actually changes
        // to account for the zero width spaces around tokens.
        let pos = $6b58de437c931efd$var$getPosition(ref.current, selection.focusNode, selection.focusOffset);
        while(true){
            let { focusNode: focusNode, focusOffset: focusOffset } = selection;
            selection.modify(extend ? 'extend' : 'move', direction, granularity);
            if (selection.focusNode === focusNode && selection.focusOffset === focusOffset) return false;
            let newPos = $6b58de437c931efd$var$getPosition(ref.current, selection.focusNode, selection.focusOffset);
            if (!$6b58de437c931efd$var$isSamePosition(pos, newPos)) return true;
        }
    };
    // macOS supports additional keyboard shortcuts for text editing.
    // We need to handle these manually so they behave consistently with tokens.
    // https://support.apple.com/en-us/102650#text
    let macShortcuts = (0, $d5a2be505488529f$export$9ac100e40613ea10)() ? {
        'Control+a': ()=>{
            return shortcuts.Home();
        },
        'Control+e': ()=>{
            return shortcuts.End();
        },
        'Control+f': ()=>{
            return shortcuts.ArrowRight();
        },
        'Control+b': ()=>{
            return shortcuts.ArrowLeft();
        }
    } : {};
    let mod = (0, $d5a2be505488529f$export$9ac100e40613ea10)() ? 'Meta' : 'Control';
    let wordModKey = (0, $d5a2be505488529f$export$9ac100e40613ea10)() ? 'Alt' : 'Control';
    let shortcuts = {
        ...macShortcuts,
        [`${mod}+z`]: ()=>{
            // If composing, the browser handles undo natively.
            if (state.isComposing) return false;
            apply((state)=>state.undo());
        },
        [(0, $d5a2be505488529f$export$9ac100e40613ea10)() ? 'Shift+Meta+z' : 'Control+y']: ()=>{
            if (state.isComposing) return false;
            apply((state)=>state.redo());
        },
        ArrowLeft: ()=>{
            return moveSelection('left', 'character');
        },
        [`${wordModKey}+ArrowLeft`]: ()=>{
            return moveSelection('left', 'word');
        },
        'Shift+ArrowLeft': ()=>{
            return moveSelection('left', 'character', true);
        },
        [`Shift+${wordModKey}+ArrowLeft`]: ()=>{
            return moveSelection('left', 'word', true);
        },
        ArrowRight: ()=>{
            return moveSelection('right', 'character');
        },
        [`${wordModKey}+ArrowRight`]: ()=>{
            return moveSelection('right', 'word');
        },
        'Shift+ArrowRight': ()=>{
            return moveSelection('right', 'character', true);
        },
        [`Shift+${wordModKey}+ArrowRight`]: ()=>{
            return moveSelection('right', 'word', true);
        },
        Home: ()=>{
            // Browsers do not behave consistently when there are tokens.
            let selection = $6b58de437c931efd$export$ca798a7e6e94638c(ref.current);
            if (!selection) return false;
            let boundary = value.findLineBoundary(selection[0], (0, $88wHG$TokenFieldValue).Direction.Backward);
            if (boundary) {
                $6b58de437c931efd$var$setCursor(ref.current, boundary, true);
                return true;
            }
            return false;
        },
        End: ()=>{
            let selection = $6b58de437c931efd$export$ca798a7e6e94638c(ref.current);
            if (!selection) return false;
            let boundary = value.findLineBoundary(selection[1], (0, $88wHG$TokenFieldValue).Direction.Forward);
            if (boundary) {
                $6b58de437c931efd$var$setCursor(ref.current, boundary, true);
                return true;
            }
            return false;
        }
    };
    // TODO: user provided onKeyDown currently relies on user provided preventDefault to stop submit
    // maybe can have them specify a format like shortcuts and merge into above?
    let { keyboardProps: keyboardProps } = (0, $bf74df7506f65576$export$8f71654801c2f7cd)({
        isDisabled: isDisabled || isReadOnly,
        onKeyDown: props.onKeyDown,
        onKeyUp: props.onKeyUp,
        shortcuts: shortcuts,
        allowRepeats: true
    });
    let { focusableProps: focusableProps } = (0, $088f27a386bc4a8f$export$4c014de7c8940b4c)(props, ref);
    let { labelProps: labelProps, fieldProps: fieldProps, descriptionProps: descriptionProps } = (0, $b5d79d79d9c34c91$export$294aa081a6c6f55d)({
        ...props,
        labelElementType: 'span'
    });
    return {
        labelProps: {
            ...labelProps,
            onClick: ()=>{
                if (!props.isDisabled) {
                    var _ref_current;
                    (_ref_current = ref.current) === null || _ref_current === void 0 ? void 0 : _ref_current.focus();
                    // Show the focus ring so the user knows where focus went
                    (0, $b50b1cc8a843ace7$export$8397ddfc504fdb9a)('keyboard');
                }
            }
        },
        descriptionProps: descriptionProps,
        tokenFieldProps: (0, $64c36edd757dfa16$export$9d1611c77c2fe928)(focusableProps, keyboardProps, fieldProps, {
            onPaste: props.onPaste,
            onCopy: props.onCopy,
            onCut: props.onCut,
            contentEditable: !isDisabled && !isReadOnly,
            suppressContentEditableWarning: true,
            role: role,
            'aria-multiline': multiline,
            'aria-details': ariaDetails,
            'aria-readonly': isReadOnly,
            'aria-disabled': isDisabled,
            style: {
                whiteSpace: 'pre-wrap'
            }
        })
    };
}
function $6b58de437c931efd$var$indexOfNode(node) {
    let index = 0;
    let n = node;
    while(n = n.previousSibling)index++;
    return index;
}
function $6b58de437c931efd$export$ca798a7e6e94638c(container) {
    let selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return null;
    let range = selection.getRangeAt(0);
    return $6b58de437c931efd$var$rangeToPositions(container, range);
}
function $6b58de437c931efd$var$rangeToPositions(container, range) {
    let start = $6b58de437c931efd$var$getPosition(container, range.startContainer, range.startOffset);
    let end = $6b58de437c931efd$var$getPosition(container, range.endContainer, range.endOffset);
    return [
        start,
        end
    ];
}
function $6b58de437c931efd$var$getPosition(container, node, offset) {
    if (node === container) return {
        index: offset,
        offset: 0
    };
    let originalNode = node;
    while(node.parentNode !== container)node = node.parentNode;
    let index = $6b58de437c931efd$var$indexOfNode(node);
    if (node.nodeType === Node.ELEMENT_NODE) {
        var _tokenNode_textContent, _node_previousSibling;
        let tokenNode = node.childNodes[1];
        let atEnd;
        let endOffset = 0;
        if (originalNode === tokenNode) // Cursor is inside the token.
        atEnd = offset > 0;
        else if (originalNode === node) // Cursor is inside the wrapper element.
        atEnd = offset > 1;
        else {
            // Cursor is on one of the zero width spaces.
            atEnd = originalNode !== tokenNode.previousSibling;
            // If the offset is greater than 1, the browser is trying to insert text into
            // the zero width space node. This will actually end up in the next text node.
            endOffset = atEnd && offset > 1 ? offset - 1 : 0;
        }
        var _tokenNode_textContent_length;
        offset = atEnd ? (_tokenNode_textContent_length = tokenNode === null || tokenNode === void 0 ? void 0 : (_tokenNode_textContent = tokenNode.textContent) === null || _tokenNode_textContent === void 0 ? void 0 : _tokenNode_textContent.length) !== null && _tokenNode_textContent_length !== void 0 ? _tokenNode_textContent_length : 0 : 0;
        // Several positions are equivalent due to the zero width spaces around tokens.
        // Normalize offset to the end of the preceding text node, or the beginning of the following node.
        if (offset === 0 && ((_node_previousSibling = node.previousSibling) === null || _node_previousSibling === void 0 ? void 0 : _node_previousSibling.nodeType) === Node.TEXT_NODE) {
            var _node_previousSibling_textContent, _node_previousSibling1;
            index--;
            var _node_previousSibling_textContent_length;
            offset = (_node_previousSibling_textContent_length = (_node_previousSibling1 = node.previousSibling) === null || _node_previousSibling1 === void 0 ? void 0 : (_node_previousSibling_textContent = _node_previousSibling1.textContent) === null || _node_previousSibling_textContent === void 0 ? void 0 : _node_previousSibling_textContent.length) !== null && _node_previousSibling_textContent_length !== void 0 ? _node_previousSibling_textContent_length : 0;
        } else if (atEnd) {
            index++;
            offset = endOffset;
        }
    }
    return {
        index: index,
        offset: offset
    };
}
let $6b58de437c931efd$var$isProgrammaticSelectionChange = Symbol('isProgrammaticSelectionChange');
function $6b58de437c931efd$var$setCursor(root, pos, fireEvent = false) {
    $6b58de437c931efd$export$9cafaa09e78b6e4b(root, pos, pos, fireEvent);
}
function $6b58de437c931efd$export$9cafaa09e78b6e4b(root, start, end, fireEvent = false) {
    let selection = window.getSelection();
    if (selection) {
        let range = $6b58de437c931efd$var$createDOMRange(root, start, end);
        root[$6b58de437c931efd$var$isProgrammaticSelectionChange] = !fireEvent;
        selection.removeAllRanges();
        selection.addRange(range);
    }
}
function $6b58de437c931efd$export$30729a8cd0694a7c(root, pos) {
    return $6b58de437c931efd$var$createDOMRange(root, pos, pos);
}
function $6b58de437c931efd$var$createDOMRange(root, start, end) {
    let range = document.createRange();
    let startChild = root.childNodes[start.index];
    if (!startChild) range.setStart(root, Math.min(root.childNodes.length, start.index));
    else if (startChild.nodeType === Node.ELEMENT_NODE) {
        // Place the cursor outside the token wrapper element.
        if (start.offset > 0) range.setStartAfter(startChild);
        else range.setStartBefore(startChild);
    } else range.setStart(startChild, start.offset);
    let endChild = root.childNodes[end.index];
    if (!endChild) range.setEnd(root, Math.min(root.childNodes.length, end.index));
    else if (endChild.nodeType === Node.ELEMENT_NODE) {
        if (end.offset > 0) range.setEndAfter(endChild);
        else range.setEndBefore(endChild);
    } else range.setEnd(endChild, end.offset);
    return range;
}
function $6b58de437c931efd$var$isSamePosition(a, b) {
    return a.index === b.index && a.offset === b.offset;
}
// Parse and validate segments from clipboard/drag data. Returns null if the data is not valid
// JSON or does not match the expected shape, so malformed or untrusted data is ignored rather
// than throwing or being inserted into the field.
function $6b58de437c931efd$var$parseSegments(json) {
    try {
        let data = JSON.parse(json);
        if (Array.isArray(data) && data.length > 0 && data.every($6b58de437c931efd$var$isValidSegment)) return data;
    } catch  {
    // Ignore invalid clipboard data.
    }
    return null;
}
function $6b58de437c931efd$var$isValidSegment(segment) {
    return typeof segment === 'object' && segment != null && (segment.type === 'text' || segment.type === 'token') && typeof segment.text === 'string';
}
function $6b58de437c931efd$var$useSelectionChange(ref, handler) {
    (0, $c3cab330536504ec$export$90fc3a17d93f704c)((0, $88wHG$useRef)(typeof document !== 'undefined' ? document : null), 'selectionchange', ()=>{
        if (ref.current && ref.current[$6b58de437c931efd$var$isProgrammaticSelectionChange]) {
            ref.current[$6b58de437c931efd$var$isProgrammaticSelectionChange] = false;
            return;
        }
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0 || !ref.current) return;
        let range = selection.getRangeAt(0);
        if (range.intersectsNode(ref.current)) handler();
    });
}
function $6b58de437c931efd$var$useMutationTracker(ref) {
    let mutationTracker = (0, $88wHG$useRef)(null);
    // Disconnect the mutation observer if the field unmounts mid-composition.
    (0, $53fed047b798be36$export$e5c5a5f917a5871c)(()=>()=>{
            var _mutationTracker_current;
            (_mutationTracker_current = mutationTracker.current) === null || _mutationTracker_current === void 0 ? void 0 : _mutationTracker_current.call(mutationTracker);
            mutationTracker.current = null;
        }, []);
    return (0, $88wHG$useMemo)(()=>({
            start () {
                var // Android sometimes fires two compositionstart events in a row, without a compositionend.
                // In that case, reuse the existing tracker.
                _mutationTracker;
                (_mutationTracker = mutationTracker).current || (_mutationTracker.current = $6b58de437c931efd$var$trackMutations(ref.current));
            },
            stop () {
                var _mutationTracker_current;
                (_mutationTracker_current = mutationTracker.current) === null || _mutationTracker_current === void 0 ? void 0 : _mutationTracker_current.call(mutationTracker);
                mutationTracker.current = null;
            }
        }), // eslint-disable-next-line react-hooks/exhaustive-deps - conflicts with compiler
    []);
}
// Tracks mutations to the DOM until the returned function is called,
// at which point the mutations are reverted.
function $6b58de437c931efd$var$trackMutations(element) {
    let mutations = [];
    let observer = new MutationObserver((records)=>{
        mutations.push(...records);
    });
    observer.observe(element, {
        childList: true,
        subtree: true,
        characterData: true,
        characterDataOldValue: true
    });
    return ()=>{
        mutations.push(...observer.takeRecords());
        observer.disconnect();
        for (let record of mutations.reverse())switch(record.type){
            case 'childList':
                for (let node of record.removedNodes)record.target.insertBefore(node, record.nextSibling);
                for (let node of record.addedNodes)record.target.removeChild(node);
                break;
            case 'characterData':
                record.target.nodeValue = record.oldValue;
                break;
        }
    };
}


export {$6b58de437c931efd$export$adf3c8de3c7afc8b as useTokenField, $6b58de437c931efd$export$ca798a7e6e94638c as getSelection, $6b58de437c931efd$export$9cafaa09e78b6e4b as setTokenFieldSelection, $6b58de437c931efd$export$30729a8cd0694a7c as tokenFieldPositionToDOMRange};
//# sourceMappingURL=useTokenField.js.map
