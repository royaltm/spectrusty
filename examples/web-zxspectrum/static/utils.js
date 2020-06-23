"use strict";

this.Spectrusty = {
  bindings: {},

  bind: function(id, event, handler, updater) {
    if (typeof event !== "string") {
      updater = handler;
      handler = event;
      event = "change";
    }

    var element = document.getElementById(id);
    element.addEventListener(event, (ev) => {
      try {
        handler(ev)
      } catch (e) {
        alert(e);
      }
    }, false);
    this.bindings[id] = updater;
    return this;
  },

  update: function() {
    for (let id in this.bindings) {
      let fun = this.bindings[id];
      if (typeof fun === "function") {
        var element = document.getElementById(id);
        fun(element)
      }
    }
    return this;
  },

  now: function() {
    return performance.now();
  }
};
