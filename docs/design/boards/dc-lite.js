// dc-lite.js: a minimal renderer for the design boards in this folder.
//
// Each board is plain HTML: an <x-dc> template plus a <script type="text/x-dc" data-dc-script> block that
// defines `class Component extends DCLogic` with renderVals(). This file implements only what the boards use:
//   {{path}} holes in text and attributes (dotted lookups and literals, never expressions),
//   <sc-for list="{{items}}" as="item"> with {{$index}}, <sc-if value="{{cond}}">,
//   onClick="{{fn}}" (any on* attribute), props from data-props defaults, state and setState().
// Query parameters override props, for example `Screen-4-Clipper.html?theme=paper&font=plex`.
//
// Licence: Apache-2.0, as the rest of the repository.
(function () {
  'use strict';

  var BOOLEAN_ATTRS = { disabled: 1, checked: 1, hidden: 1, selected: 1, readonly: 1, required: 1, open: 1, multiple: 1 };
  var WHOLE = /^\s*\{\{\s*([^{}]+?)\s*\}\}\s*$/;
  var HOLE = /\{\{\s*([^{}]+?)\s*\}\}/g;

  function DCLogic(props) {
    this.props = props || {};
    this.state = {};
  }
  DCLogic.prototype.setState = function (patch) {
    var next = typeof patch === 'function' ? patch(this.state, this.props) : patch;
    for (var k in next) if (Object.prototype.hasOwnProperty.call(next, k)) this.state[k] = next[k];
    if (this.__render) this.__render();
  };
  DCLogic.prototype.forceUpdate = function () {
    if (this.__render) this.__render();
  };
  DCLogic.prototype.renderVals = function () {
    return {};
  };

  function literal(expr) {
    if (expr === 'true') return { v: true };
    if (expr === 'false') return { v: false };
    if (expr === 'null') return { v: null };
    if (/^-?\d+(\.\d+)?$/.test(expr)) return { v: Number(expr) };
    var m = /^(['"])(.*)\1$/.exec(expr);
    if (m) return { v: m[2] };
    return null;
  }

  function lookup(expr, scopes) {
    var lit = literal(expr);
    if (lit) return lit.v;
    var parts = expr.split('.');
    for (var i = scopes.length - 1; i >= 0; i--) {
      var s = scopes[i];
      if (s != null && Object.prototype.hasOwnProperty.call(s, parts[0])) {
        var v = s[parts[0]];
        for (var j = 1; j < parts.length && v != null; j++) v = v[parts[j]];
        return v;
      }
    }
    return undefined;
  }

  function interpolate(text, scopes) {
    return text.replace(HOLE, function (_, expr) {
      var v = lookup(expr, scopes);
      return v == null ? '' : String(v);
    });
  }

  function setAttr(el, name, raw, scopes) {
    if (name.indexOf('hint-placeholder') === 0) return;
    var whole = WHOLE.exec(raw);
    if (whole) {
      var v = lookup(whole[1], scopes);
      if (name.slice(0, 2) === 'on' && name.length > 2) {
        if (typeof v === 'function') el.addEventListener(name.slice(2).toLowerCase(), v);
        return;
      }
      if (BOOLEAN_ATTRS[name]) {
        if (v === true || v === 'true') el.setAttribute(name, '');
        return;
      }
      if (v == null || v === false && name.indexOf('aria-') !== 0) return;
      el.setAttribute(name, String(v));
      if (name === 'value' && 'value' in el) el.value = String(v);
      return;
    }
    el.setAttribute(name, raw.indexOf('{{') >= 0 ? interpolate(raw, scopes) : raw);
  }

  function renderChildren(src, parent, scopes) {
    for (var n = src.firstChild; n; n = n.nextSibling) renderNode(n, parent, scopes);
  }

  function renderNode(node, parent, scopes) {
    if (node.nodeType === 3) {
      var t = node.nodeValue;
      parent.appendChild(document.createTextNode(t.indexOf('{{') >= 0 ? interpolate(t, scopes) : t));
      return;
    }
    if (node.nodeType !== 1) return;
    var tag = node.localName;
    if (tag === 'sc-for') {
      var listExpr = WHOLE.exec(node.getAttribute('list') || '');
      var list = listExpr ? lookup(listExpr[1], scopes) : null;
      var as = node.getAttribute('as') || 'item';
      if (!list || !list.length) return;
      for (var i = 0; i < list.length; i++) {
        var frame = { $index: i };
        frame[as] = list[i];
        renderChildren(node, parent, scopes.concat([frame]));
      }
      return;
    }
    if (tag === 'sc-if') {
      var condExpr = WHOLE.exec(node.getAttribute('value') || '');
      if (condExpr && lookup(condExpr[1], scopes)) renderChildren(node, parent, scopes);
      return;
    }
    var el = node.namespaceURI && node.namespaceURI !== 'http://www.w3.org/1999/xhtml'
      ? document.createElementNS(node.namespaceURI, node.tagName)
      : document.createElement(tag);
    for (var a = 0; a < node.attributes.length; a++) setAttr(el, node.attributes[a].name, node.attributes[a].value, scopes);
    renderChildren(tag === 'template' ? node.content : node, el, scopes);
    parent.appendChild(el);
  }

  function propsFrom(script) {
    var props = {};
    var decl = {};
    try {
      decl = JSON.parse(script.getAttribute('data-props') || '{}');
    } catch (e) {
      console.error('dc-lite: data-props is not valid JSON', e);
    }
    for (var k in decl) if (k.charAt(0) !== '$' && decl[k] && 'default' in decl[k]) props[k] = decl[k].default;
    var q = new URLSearchParams(window.location.search);
    q.forEach(function (v, k) {
      props[k] = v;
    });
    return props;
  }

  function mount(host, script) {
    var helmet = host.querySelector('helmet');
    if (helmet) {
      while (helmet.firstChild) document.head.appendChild(helmet.firstChild);
      helmet.parentNode.removeChild(helmet);
    }
    var tpl = document.createDocumentFragment();
    while (host.firstChild) tpl.appendChild(host.firstChild);

    var Component = DCLogic;
    if (script) {
      Component = new Function('DCLogic', script.textContent + '\n;return typeof Component === "function" ? Component : DCLogic;')(DCLogic);
    }
    var inst = new Component(script ? propsFrom(script) : {});
    if (!inst.state) inst.state = {};
    inst.__render = function () {
      var vals = inst.renderVals() || {};
      var out = document.createDocumentFragment();
      renderChildren(tpl, out, [vals]);
      host.replaceChildren(out);
    };
    inst.__render();
    host.setAttribute('data-ready', '');
  }

  var style = document.createElement('style');
  style.textContent = 'x-dc{display:block}x-dc:not([data-ready]){visibility:hidden}';
  document.head.appendChild(style);

  document.addEventListener('DOMContentLoaded', function () {
    var hosts = document.querySelectorAll('x-dc');
    var scripts = document.querySelectorAll('script[data-dc-script]');
    for (var i = 0; i < hosts.length; i++) mount(hosts[i], scripts[i] || null);
  });
})();
