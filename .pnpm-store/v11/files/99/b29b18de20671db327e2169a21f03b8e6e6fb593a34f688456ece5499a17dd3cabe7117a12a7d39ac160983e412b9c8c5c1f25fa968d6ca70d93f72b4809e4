'use client';
import * as React from 'react';
import NumberFlowLite, { define, formatToData, renderInnerHTML } from 'number-flow/lite';
import { BROWSER } from 'esm-env';

const REACT_MAJOR = parseInt(React.version.match(/^(\d+)\./)?.[1]);
const isReact19 = REACT_MAJOR >= 19;
// Can't wait to not have to do this in React 19:
const OBSERVED_ATTRIBUTES = [
    'data',
    'digits'
];
class NumberFlowElement extends NumberFlowLite {
    attributeChangedCallback(attr, _oldValue, newValue) {
        this[attr] = JSON.parse(newValue);
    }
}
NumberFlowElement.observedAttributes = isReact19 ? [] : OBSERVED_ATTRIBUTES;
define('number-flow-react', NumberFlowElement);
// You're supposed to cache these between uses:
// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number/toLocaleString
// Serialize to strings b/c React:
const formatters = {};
// Tiny workaround to support React 19 until it's released:
function identity(v) {
    return v;
}
const serialize = isReact19 ? identity : JSON.stringify;
function splitProps(props) {
    const { transformTiming, spinTiming, opacityTiming, animated, respectMotionPreference, trend, plugins, ...rest } = props;
    return [
        {
            transformTiming,
            spinTiming,
            opacityTiming,
            animated,
            respectMotionPreference,
            trend,
            plugins
        },
        rest
    ];
}
// We need a class component to use getSnapshotBeforeUpdate:
class NumberFlowImpl extends React.Component {
    // Update the non-`data` props to avoid JSON serialization
    // Data needs to be set in render still:
    updateProperties(prevProps) {
        if (!this.el) return;
        this.el.batched = !this.props.isolate;
        const [nonData] = splitProps(this.props);
        Object.entries(nonData).forEach(([k, v])=>{
            // @ts-ignore
            this.el[k] = v ?? NumberFlowElement.defaultProps[k];
        });
        if (prevProps?.onAnimationsStart) this.el.removeEventListener('animationsstart', prevProps.onAnimationsStart);
        if (this.props.onAnimationsStart) this.el.addEventListener('animationsstart', this.props.onAnimationsStart);
        if (prevProps?.onAnimationsFinish) this.el.removeEventListener('animationsfinish', prevProps.onAnimationsFinish);
        if (this.props.onAnimationsFinish) this.el.addEventListener('animationsfinish', this.props.onAnimationsFinish);
    }
    componentDidMount() {
        this.updateProperties();
        if (isReact19 && this.el) {
            // React 19 needs this because the attributeChangedCallback isn't called:
            this.el.digits = this.props.digits;
            this.el.data = this.props.data;
        }
    }
    getSnapshotBeforeUpdate(prevProps) {
        this.updateProperties(prevProps);
        if (prevProps.data !== this.props.data) {
            if (this.props.group) {
                this.props.group.willUpdate();
                return ()=>this.props.group?.didUpdate();
            }
            if (!this.props.isolate) {
                this.el?.willUpdate();
                return ()=>this.el?.didUpdate();
            }
        }
        return null;
    }
    componentDidUpdate(_, __, didUpdate) {
        didUpdate?.();
    }
    handleRef(el) {
        if (this.props.innerRef) this.props.innerRef.current = el;
        this.el = el;
    }
    render() {
        const [_, { innerRef, className, data, nonce, willChange, isolate, group, digits, onAnimationsStart, onAnimationsFinish, ...rest }] = splitProps(this.props);
        return(// @ts-expect-error missing types
        /*#__PURE__*/ React.createElement("number-flow-react", {
            ref: this.handleRef,
            "data-will-change": willChange ? '' : undefined,
            // Have to rename this:
            class: className,
            nonce: nonce,
            ...rest,
            dangerouslySetInnerHTML: {
                __html: BROWSER ? '' : renderInnerHTML(data, {
                    nonce,
                    elementSuffix: '-react'
                })
            },
            suppressHydrationWarning: true,
            digits: serialize(digits),
            // Make sure data is set last, everything else is updated:
            data: serialize(data)
        }));
    }
    constructor(props){
        super(props);
        this.handleRef = this.handleRef.bind(this);
    }
}
const NumberFlow = /*#__PURE__*/ React.forwardRef(function NumberFlow({ value, locales, format, prefix, suffix, ...props }, _ref) {
    React.useImperativeHandle(_ref, ()=>ref.current, []);
    const ref = React.useRef(undefined);
    const group = React.useContext(NumberFlowGroupContext);
    group?.useRegister(ref);
    const localesString = React.useMemo(()=>locales ? JSON.stringify(locales) : '', [
        locales
    ]);
    const formatString = React.useMemo(()=>format ? JSON.stringify(format) : '', [
        format
    ]);
    const data = React.useMemo(()=>{
        const formatter = formatters[`${localesString}:${formatString}`] ??= new Intl.NumberFormat(locales, format);
        return formatToData(value, formatter, prefix, suffix);
    }, [
        value,
        localesString,
        formatString,
        prefix,
        suffix
    ]);
    return /*#__PURE__*/ React.createElement(NumberFlowImpl, {
        ...props,
        group: group,
        data: data,
        innerRef: ref
    });
});
const NumberFlowGroupContext = /*#__PURE__*/ React.createContext(undefined);
function NumberFlowGroup({ children }) {
    const flows = React.useRef(new Set());
    const updating = React.useRef(false);
    const pending = React.useRef(new WeakMap());
    const value = React.useMemo(()=>({
            useRegister (ref) {
                React.useEffect(()=>{
                    flows.current.add(ref);
                    return ()=>{
                        flows.current.delete(ref);
                    };
                }, []);
            },
            willUpdate () {
                if (updating.current) return;
                updating.current = true;
                flows.current.forEach((ref)=>{
                    const f = ref.current;
                    if (!f || !f.created) return;
                    f.willUpdate();
                    pending.current.set(f, true);
                });
            },
            didUpdate () {
                flows.current.forEach((ref)=>{
                    const f = ref.current;
                    if (!f || !pending.current.get(f)) return;
                    f.didUpdate();
                    pending.current.delete(f);
                });
                updating.current = false;
            }
        }), []);
    return /*#__PURE__*/ React.createElement(NumberFlowGroupContext.Provider, {
        value: value
    }, children);
}

export { NumberFlow as N, NumberFlowElement as a, NumberFlowGroup as b };
