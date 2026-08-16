import u, { define as l, formatToData as r } from "./lite.mjs";
import { Digit as N, canAnimate as T } from "./lite.mjs";
import { r as m } from "./ssr-DvIINv8w.mjs";
import { p as L } from "./ssr-DvIINv8w.mjs";
import { buildStyles as c } from "./csp.mjs";
import { continuous as I } from "./plugins.mjs";
const v = c(), f = "number-flow-connect", h = "number-flow-update", E = (e, { locales: t, format: s, numberPrefix: n, numberSuffix: o, nonce: i } = {}) => {
  const a = r(e, new Intl.NumberFormat(t, s), n, o);
  return m(a, { nonce: i });
};
class b extends u {
  constructor() {
    super(...arguments), this.connected = !1;
  }
  connectedCallback() {
    this.connected = !0, this.dispatchEvent(new Event(f, { bubbles: !0 }));
  }
  disconnectedCallback() {
    this.connected = !1;
  }
  get value() {
    return this._value;
  }
  update(t) {
    (!this._formatter || this._prevFormat !== this.format || this._prevLocales !== this.locales) && (this._formatter = new Intl.NumberFormat(this.locales, this.format), this._prevFormat = this.format, this._prevLocales = this.locales), t != null && (this._value = t), this.dispatchEvent(new Event(h, { bubbles: !0 })), this.data = r(this._value, this._formatter, this.numberPrefix, this.numberSuffix);
  }
}
l("number-flow", b);
export {
  f as CONNECT_EVENT,
  N as Digit,
  h as UPDATE_EVENT,
  T as canAnimate,
  I as continuous,
  b as default,
  l as define,
  r as formatToData,
  L as prefersReducedMotion,
  E as renderInnerHTML,
  v as styles
};
