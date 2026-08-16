Object.defineProperty(exports, '__esModule', { value: true });

var React = require('react');
var NumberFlowLite = require('number-flow/lite');
var csp = require('number-flow/csp');
var plugins = require('number-flow/plugins');
var NumberFlowClient = require('./NumberFlow-client-Cwo3BD4J.js');

function _interopNamespace(e) {
	if (e && e.__esModule) return e;
	var n = Object.create(null);
	if (e) {
		Object.keys(e).forEach(function (k) {
			if (k !== 'default') {
				var d = Object.getOwnPropertyDescriptor(e, k);
				Object.defineProperty(n, k, d.get ? d : {
					enumerable: true,
					get: function () { return e[k]; }
				});
			}
		});
	}
	n.default = e;
	return n;
}

var React__namespace = /*#__PURE__*/_interopNamespace(React);

const styles = csp.buildStyles('-react');
const useIsSupported = ()=>React__namespace.useSyncExternalStore(()=>()=>{}, ()=>NumberFlowLite.canAnimate, ()=>false);
const usePrefersReducedMotion = ()=>React__namespace.useSyncExternalStore((cb)=>{
        NumberFlowLite.prefersReducedMotion?.addEventListener('change', cb);
        return ()=>NumberFlowLite.prefersReducedMotion?.removeEventListener('change', cb);
    }, ()=>NumberFlowLite.prefersReducedMotion.matches, ()=>false);
function useCanAnimate({ respectMotionPreference = true } = {}) {
    const isSupported = useIsSupported();
    const reducedMotion = usePrefersReducedMotion();
    return isSupported && (!respectMotionPreference || !reducedMotion);
}

exports.NumberFlowElement = NumberFlowClient.NumberFlowElement;
exports.NumberFlowGroup = NumberFlowClient.NumberFlowGroup;
exports.default = NumberFlowClient.NumberFlow;
exports.styles = styles;
exports.useCanAnimate = useCanAnimate;
exports.useIsSupported = useIsSupported;
exports.usePrefersReducedMotion = usePrefersReducedMotion;
Object.keys(plugins).forEach(function (k) {
	if (k !== 'default' && !Object.prototype.hasOwnProperty.call(exports, k)) Object.defineProperty(exports, k, {
		enumerable: true,
		get: function () { return plugins[k]; }
	});
});
