import { BROWSER as j } from "esm-env";
import { c as S, d as T, e as L, S as M, b as P, p as w, f as N, w as $, g as V, o as b } from "./ssr-DvIINv8w.mjs";
import { r as q } from "./ssr-DvIINv8w.mjs";
import { continuous as J } from "./plugins.mjs";
const f = (n, t, e) => {
  const i = document.createElement(n), [s, o] = Array.isArray(t) ? [void 0, t] : [t, e];
  return s && Object.assign(i, s), o == null || o.forEach((a) => i.appendChild(a)), i;
}, D = (n, t) => {
  var e;
  return t === "left" ? n.offsetLeft : (((e = n.offsetParent instanceof HTMLElement ? n.offsetParent : null) == null ? void 0 : e.offsetWidth) ?? 0) - n.offsetWidth - n.offsetLeft;
}, W = (n) => n.offsetWidth > 0 && n.offsetHeight > 0, X = (n, t) => {
  j && typeof HTMLElement < "u" && typeof customElements < "u" && !customElements.get(n) && customElements.define(n, t);
};
function k(n, t, { reverse: e = !1 } = {}) {
  const i = n.length;
  for (let s = e ? i - 1 : 0; e ? s >= 0 : s < i; e ? s-- : s++)
    t(n[s], s);
}
function z(n, t, e, i) {
  const s = t.formatToParts(n);
  e && s.unshift({ type: "prefix", value: e }), i && s.push({ type: "suffix", value: i });
  const o = [], a = [], r = [], d = [], c = {}, p = (l) => `${l}:${c[l] = (c[l] ?? -1) + 1}`;
  let u = "", m = !1, g = !1;
  for (const l of s) {
    u += l.value;
    const h = l.type === "minusSign" || l.type === "plusSign" ? "sign" : l.type;
    h === "integer" ? (m = !0, a.push(...l.value.split("").map((_) => ({ type: h, value: parseInt(_) })))) : h === "group" ? a.push({ type: h, value: l.value }) : h === "decimal" ? (g = !0, r.push({ type: h, value: l.value, key: p(h) })) : h === "fraction" ? r.push(...l.value.split("").map((_) => ({
      type: h,
      value: parseInt(_),
      key: p(h),
      pos: -1 - c[h]
    }))) : (m || g ? d : o).push({
      type: h,
      value: l.value,
      key: p(h)
    });
  }
  const v = [];
  for (let l = a.length - 1; l >= 0; l--) {
    const h = a[l];
    v.unshift(h.type === "integer" ? {
      ...h,
      key: p(h.type),
      pos: c[h.type]
    } : {
      ...h,
      key: p(h.type)
    });
  }
  return {
    pre: o,
    integer: v,
    fraction: r,
    post: d,
    valueAsString: u,
    value: typeof n == "string" ? parseFloat(n) : n
  };
}
const E = S && T && L;
class B extends M {
  constructor() {
    super(), this.created = !1, this.batched = !1, this._preUpdated = !1;
    const { animated: t, ...e } = this.constructor.defaultProps;
    this._animated = this.computedAnimated = t, Object.assign(this, e);
  }
  get animated() {
    return this._animated;
  }
  set animated(t) {
    var e;
    this.animated !== t && (this._animated = t, (e = this.shadowRoot) == null || e.getAnimations().forEach((i) => i.finish()));
  }
  /**
   * @internal
   */
  set data(t) {
    var r, d;
    if (t == null || t === this._data)
      return;
    const { pre: e, integer: i, fraction: s, post: o, value: a } = t;
    if (this.created) {
      const c = this._data;
      this._data = t, this.computedTrend = typeof this.trend == "function" ? this.trend(c.value, a) : this.trend, this.computedAnimated = E && this._animated && (!this.respectMotionPreference || !((r = w) != null && r.matches)) && // https://github.com/barvian/number-flow/issues/9
      W(this) && // https://github.com/barvian/number-flow/issues/165
      this.ownerDocument.visibilityState === "visible", (d = this.plugins) == null || d.forEach((p) => {
        var u;
        return (u = p.onUpdate) == null ? void 0 : u.call(p, t, c, this);
      }), this.batched || this.willUpdate(), this._pre.update(e), this._num.update({ integer: i, fraction: s }), this._post.update(o), this.batched || this.didUpdate();
    } else {
      this._data = t, this.attachShadow({ mode: "open" });
      try {
        this._internals ?? (this._internals = this.attachInternals()), this._internals.role = "img";
      } catch {
      }
      const c = document.createElement("style");
      this.nonce && (c.nonce = this.nonce), c.textContent = P, this.shadowRoot.appendChild(c), this._pre = new U(this, e, {
        justify: "right",
        part: "left"
      }), this.shadowRoot.appendChild(this._pre.el), this._num = new F(this, i, s), this.shadowRoot.appendChild(this._num.el), this._post = new U(this, o, {
        justify: "left",
        part: "right"
      }), this.shadowRoot.appendChild(this._post.el), this.created = !0;
    }
    try {
      this._internals.ariaLabel = t.valueAsString;
    } catch {
    }
  }
  /**
   * @internal
   */
  willUpdate() {
    var t;
    this._preUpdated = E && this._animated && (!this.respectMotionPreference || !((t = w) != null && t.matches)) && this.ownerDocument.visibilityState === "visible", this._preUpdated && (this._pre.willUpdate(), this._num.willUpdate(), this._post.willUpdate());
  }
  /**
   * @internal
   */
  didUpdate() {
    if (!this.computedAnimated || !this._preUpdated)
      return;
    this._abortAnimationsFinish ? this._abortAnimationsFinish.abort() : this.dispatchEvent(new Event("animationsstart")), this._pre.didUpdate(), this._num.didUpdate(), this._post.didUpdate();
    const t = new AbortController();
    Promise.all(this.shadowRoot.getAnimations().map((e) => e.finished)).then(() => {
      t.signal.aborted || (this.dispatchEvent(new Event("animationsfinish")), this._abortAnimationsFinish = void 0);
    }), this._abortAnimationsFinish = t;
  }
}
B.defaultProps = {
  transformTiming: {
    duration: 900,
    // Make sure to keep this minified:
    easing: "linear(0,.005,.019,.039,.066,.096,.129,.165,.202,.24,.278,.316,.354,.39,.426,.461,.494,.526,.557,.586,.614,.64,.665,.689,.711,.731,.751,.769,.786,.802,.817,.831,.844,.856,.867,.877,.887,.896,.904,.912,.919,.925,.931,.937,.942,.947,.951,.955,.959,.962,.965,.968,.971,.973,.976,.978,.98,.981,.983,.984,.986,.987,.988,.989,.99,.991,.992,.992,.993,.994,.994,.995,.995,.996,.996,.9963,.9967,.9969,.9972,.9975,.9977,.9979,.9981,.9982,.9984,.9985,.9987,.9988,.9989,1)"
  },
  spinTiming: void 0,
  opacityTiming: { duration: 450, easing: "ease-out" },
  animated: !0,
  trend: (n, t) => Math.sign(t - n),
  respectMotionPreference: !0,
  plugins: void 0,
  digits: void 0
};
class F {
  constructor(t, e, i, { className: s, ...o } = {}) {
    this.flow = t, this._integer = new A(t, e, {
      justify: "right",
      part: "integer"
    }), this._fraction = new A(t, i, {
      justify: "left",
      part: "fraction"
    }), this._inner = f("span", {
      className: "number__inner"
    }, [this._integer.el, this._fraction.el]), this.el = f("span", {
      ...o,
      part: "number",
      className: `number ${s ?? ""}`
    }, [this._inner]);
  }
  willUpdate() {
    this._prevWidth = this.el.offsetWidth, this._prevLeft = this.el.getBoundingClientRect().left, this._integer.willUpdate(), this._fraction.willUpdate();
  }
  update({ integer: t, fraction: e }) {
    this._integer.update(t), this._fraction.update(e);
  }
  didUpdate() {
    const t = this.el.getBoundingClientRect();
    this._integer.didUpdate(), this._fraction.didUpdate();
    const e = this._prevLeft - t.left, i = this.el.offsetWidth, s = this._prevWidth - i;
    this.el.style.setProperty("--width", String(i)), this.el.animate({
      [N]: [`${e}px`, "0px"],
      [$]: [s, 0]
    }, {
      ...this.flow.transformTiming,
      composite: "accumulate"
    });
  }
}
class R {
  constructor(t, e, { justify: i, className: s, ...o }, a) {
    this.flow = t, this.children = /* @__PURE__ */ new Map(), this.onCharRemove = (d) => () => {
      this.children.delete(d);
    }, this.justify = i;
    const r = e.map((d) => this.addChar(d).el);
    this.el = f("span", {
      ...o,
      className: `section section--justify-${i} ${s ?? ""}`
    }, a ? a(r) : r);
  }
  addChar(t, { startDigitsAtZero: e = !1, ...i } = {}) {
    const s = t.type === "integer" || t.type === "fraction" ? new C(this, t.type, e ? 0 : t.value, t.pos, {
      ...i,
      onRemove: this.onCharRemove(t.key)
    }) : new I(this, t.type, t.value, {
      ...i,
      onRemove: this.onCharRemove(t.key)
    });
    return this.children.set(t.key, s), s;
  }
  unpop(t) {
    t.el.removeAttribute("inert"), t.el.style.top = "", t.el.style[this.justify] = "";
  }
  pop(t) {
    t.forEach((e) => {
      e.el.style.top = `${e.el.offsetTop}px`, e.el.style[this.justify] = `${D(e.el, this.justify)}px`;
    }), t.forEach((e) => {
      e.el.setAttribute("inert", ""), e.present = !1;
    });
  }
  addNewAndUpdateExisting(t) {
    const e = /* @__PURE__ */ new Map(), i = /* @__PURE__ */ new Map(), s = this.justify === "left", o = s ? "prepend" : "append";
    if (k(t, (a) => {
      let r;
      this.children.has(a.key) ? (r = this.children.get(a.key), i.set(a, r), this.unpop(r), r.present = !0) : (r = this.addChar(a, { startDigitsAtZero: !0, animateIn: !0 }), e.set(a, r)), this.el[o](r.el);
    }, { reverse: s }), this.flow.computedAnimated) {
      const a = this.el.getBoundingClientRect();
      e.forEach((r) => {
        r.willUpdate(a);
      });
    }
    e.forEach((a, r) => {
      a.update(r.value);
    }), i.forEach((a, r) => {
      a.update(r.value);
    });
  }
  willUpdate() {
    const t = this.el.getBoundingClientRect();
    this._prevOffset = t[this.justify], this.children.forEach((e) => e.willUpdate(t));
  }
  didUpdate() {
    const t = this.el.getBoundingClientRect();
    this.children.forEach((s) => s.didUpdate(t));
    const e = t[this.justify], i = this._prevOffset - e;
    i && this.children.size && this.el.animate({
      transform: [`translateX(${i}px)`, "none"]
    }, {
      ...this.flow.transformTiming,
      composite: "accumulate"
    });
  }
}
class A extends R {
  update(t) {
    const e = /* @__PURE__ */ new Map();
    this.children.forEach((i, s) => {
      t.find((o) => o.key === s) || e.set(s, i), this.unpop(i);
    }), this.addNewAndUpdateExisting(t), e.forEach((i) => {
      i instanceof C && i.update(0);
    }), this.pop(e);
  }
}
class U extends R {
  update(t) {
    const e = /* @__PURE__ */ new Map();
    this.children.forEach((i, s) => {
      t.find((o) => o.key === s) || e.set(s, i);
    }), this.pop(e), this.addNewAndUpdateExisting(t);
  }
}
class y {
  constructor(t, e, { onRemove: i, animateIn: s = !1 } = {}) {
    this.flow = t, this.el = e, this._present = !0, this._remove = () => {
      var o;
      this.el.remove(), (o = this._onRemove) == null || o.call(this);
    }, this.el.classList.add("animate-presence"), this.flow.computedAnimated && s && this.el.animate({
      [b]: [-0.9999, 0]
    }, {
      ...this.flow.opacityTiming,
      composite: "accumulate"
    }), this._onRemove = i;
  }
  get present() {
    return this._present;
  }
  set present(t) {
    if (this._present !== t) {
      if (this._present = t, t ? this.el.removeAttribute("inert") : this.el.setAttribute("inert", ""), !this.flow.computedAnimated) {
        t || this._remove();
        return;
      }
      this.el.style.setProperty("--_number-flow-d-opacity", t ? "0" : "-.999"), this.el.animate({
        [b]: t ? [-0.9999, 0] : [0.999, 0]
      }, {
        ...this.flow.opacityTiming,
        composite: "accumulate"
      }), t ? this.flow.removeEventListener("animationsfinish", this._remove) : this.flow.addEventListener("animationsfinish", this._remove, {
        once: !0
      });
    }
  }
}
class x extends y {
  constructor(t, e, i, s) {
    super(t.flow, i, s), this.section = t, this.value = e, this.el = i;
  }
}
class C extends x {
  constructor(t, e, i, s, o) {
    var c, p;
    const a = (((p = (c = t.flow.digits) == null ? void 0 : c[s]) == null ? void 0 : p.max) ?? 9) + 1, r = Array.from({ length: a }).map((u, m) => {
      const g = f("span", { className: "digit__num" }, [
        document.createTextNode(String(m))
      ]);
      return m !== i && g.setAttribute("inert", ""), g.style.setProperty("--n", String(m)), g;
    }), d = f("span", {
      part: `digit ${e}-digit`,
      className: "digit"
    }, r);
    d.style.setProperty("--current", String(i)), d.style.setProperty("--length", String(a)), super(t, i, d, o), this.pos = s, this._onAnimationsFinish = () => {
      this.el.classList.remove("is-spinning");
    }, this._numbers = r, this.length = a;
  }
  willUpdate(t) {
    const e = this.el.getBoundingClientRect();
    this._prevValue = this.value;
    const i = e[this.section.justify] - t[this.section.justify], s = e.width / 2;
    this._prevCenter = this.section.justify === "left" ? i + s : i - s;
  }
  update(t) {
    this.el.style.setProperty("--current", String(t)), this._numbers.forEach((e, i) => i === t ? e.removeAttribute("inert") : e.setAttribute("inert", "")), this.value = t;
  }
  didUpdate(t) {
    const e = this.el.getBoundingClientRect(), i = e[this.section.justify] - t[this.section.justify], s = e.width / 2, o = this.section.justify === "left" ? i + s : i - s, a = this._prevCenter - o;
    a && this.el.animate({
      transform: [`translateX(${a}px)`, "none"]
    }, {
      ...this.flow.transformTiming,
      composite: "accumulate"
    });
    const r = this.getDelta();
    r && (this.el.classList.add("is-spinning"), this.el.animate({
      [V]: [-r, 0]
    }, {
      ...this.flow.spinTiming ?? this.flow.transformTiming,
      composite: "accumulate"
    }), this.flow.addEventListener("animationsfinish", this._onAnimationsFinish, { once: !0 }));
  }
  getDelta() {
    var i;
    if (this.flow.plugins)
      for (const s of this.flow.plugins) {
        const o = (i = s.getDelta) == null ? void 0 : i.call(s, this.value, this._prevValue, this);
        if (o != null)
          return o;
      }
    const t = this.value - this._prevValue, e = this.flow.computedTrend || Math.sign(t);
    return e < 0 && this.value > this._prevValue ? this.value - this.length - this._prevValue : e > 0 && this.value < this._prevValue ? this.length - this._prevValue + this.value : t;
  }
}
class I extends x {
  constructor(t, e, i, s) {
    const o = f("span", {
      className: "symbol__value",
      textContent: i
    });
    super(t, i, f("span", {
      part: `symbol ${e}`,
      className: "symbol"
    }, [o]), s), this.type = e, this._children = /* @__PURE__ */ new Map(), this._onChildRemove = (a) => () => {
      this._children.delete(a);
    }, this._children.set(i, new y(this.flow, o, {
      onRemove: this._onChildRemove(i)
    }));
  }
  willUpdate(t) {
    if (this.type === "decimal")
      return;
    const e = this.el.getBoundingClientRect();
    this._prevOffset = e[this.section.justify] - t[this.section.justify];
  }
  update(t) {
    if (this.value !== t) {
      const e = this._children.get(this.value);
      e && (e.present = !1);
      const i = this._children.get(t);
      if (i)
        i.present = !0;
      else {
        const s = f("span", {
          className: "symbol__value",
          textContent: t
        });
        this.el.appendChild(s), this._children.set(t, new y(this.flow, s, {
          animateIn: !0,
          onRemove: this._onChildRemove(t)
        }));
      }
    }
    this.value = t;
  }
  didUpdate(t) {
    if (this.type === "decimal")
      return;
    const i = this.el.getBoundingClientRect()[this.section.justify] - t[this.section.justify], s = this._prevOffset - i;
    s && this.el.animate({
      transform: [`translateX(${s}px)`, "none"]
    }, { ...this.flow.transformTiming, composite: "accumulate" });
  }
}
export {
  C as Digit,
  E as canAnimate,
  J as continuous,
  B as default,
  X as define,
  z as formatToData,
  w as prefersReducedMotion,
  q as renderInnerHTML
};
