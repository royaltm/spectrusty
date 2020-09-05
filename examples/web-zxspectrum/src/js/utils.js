export class UserInterface {
  constructor() {
    this.bindings = {};
  }

  bind(id, event, handler, updater) {
    if (typeof event !== "string") {
      updater = handler;
      handler = event;
      event = "change";
    }

    var element = document.getElementById(id);
    element.addEventListener(event, (ev) => {
      ev.preventDefault();
      try {
        handler(ev);
        if (typeof this.onchange === 'function') {
          this.onchange(id);
        }
      } catch (e) {
        alert(e);
      }
    }, false);
    this.bindings[id] = updater;
    return this;
  }

  update() {
    for (let id in this.bindings) {
      let fun = this.bindings[id];
      if (typeof fun === "function") {
        var element = document.getElementById(id);
        fun(element)
      }
    }
    return this;
  }
}

export function cpuFactor(x) {
  return (x >= 0) ? 1 + x/100
                  : 1 / (1 - x/100)
}

export function cpuSlider(y) {
  return (y >= 1) ? (y - 1)*100
                  : (1 - 1 / y)*100
}

const RANGE_RE = /^\s*(0x[0-9a-f]+|\d+)\s*([,=:])\s*(0x[0-9a-f]+|\d+)\s*$/i;

export function parseRange(s) {
  var res = s.match(RANGE_RE);
  if (res) {
    let start = parseInt(res[1]),
        end = parseInt(res[3]);
    if (isFinite(start) && isFinite(end)) {
      switch(res[2]) {
        case ',': return [start, start + end];
        case ':': return [start, end];
      }
    }
  }
}

export function parsePoke(s) {
  var res = s.match(RANGE_RE);
  if (res) {
    let start = parseInt(res[1]),
        val = parseInt(res[3]);
    if (isFinite(start) && isFinite(val)) {
      switch(res[2]) {
        case ',': case '=': return [start, val];
      }
    }
  }
}

export function toHex(n, pad) {
  var nstr = n.toString(16);
  pad -= nstr.length;
  return "0".repeat(pad >= 0 ? pad : 0) + nstr;
}

export function $id(el) {
  return document.getElementById(el);
}

export function $on(el, ev, cb) {
  el.addEventListener(ev, cb, false);
}

export function dowloader() {
  const saver = document.createElement("a");
  document.body.appendChild(saver);
  saver.style = "display: none";

  return function downloadFile(data, mime, name) {
    var blob = new Blob([data], {type: mime})
    var url = URL.createObjectURL(blob);
    saver.href = url;
    saver.download = name;
    saver.click();
    URL.revokeObjectURL(url);
  }
}

export function setupDevice(spectrum, name, attach) {
  if (attach) {
    spectrum.attachDevice(name)
  }
  else {
    spectrum.detachDevice(name)
  }
}

export function loadRemote(uri, asText) {
  return fetch(uri).then(response => {
    if (response.ok) {
      return asText ? response.text()
                    : response.arrayBuffer().then(buffer => new Uint8Array(buffer));
    }
    else {
      throw "Error loading from: " + uri;
    }
  })
}

const FRESH_TEST = /#fresh(?:#|$)/;

export function stateGuard(spectrum, urlparams) {
  const storage = window.localStorage,
        GRACE_TIME = 1000;
  var lastTimeStamp = 0;
  return function stateSave(event) {
    var timeStamp = event.timeStamp;
    if (storage != null && timeStamp > lastTimeStamp + GRACE_TIME) {
      lastTimeStamp = timeStamp;
      let key = urlparams.cache;
      if (FRESH_TEST.test(key)) {
        storage.clear();
      }
      else {
        let value = spectrum.toJSON();
        storage.clear();
        storage.setItem(key, value);
      }
    }
  }
}

export function restoreState(spectrum, urlparams) {
  const storage = window.localStorage,
        key = urlparams.cache;
  if (storage != null && !FRESH_TEST.test(key)) {
    let json = storage.getItem(key);
    if (json != null) {
      try {
        spectrum.parseJSON(json);
        return true;
      }
      catch(e) {
        console.error(e);
      }
    }
  }
  return false;
}

export function checkBrowserCapacity() {
  var alert = $id('alert');
  try {
    /* check edge features */
    if ('undefined' === typeof window.WebAssembly) {
      throw Error("required browser with WebAssembly support");
    }
    if ('function' !== typeof window.requestAnimationFrame) {
      throw Error("required browser with requestAnimationFrame support");
    }
    if ('function' !== typeof window.fetch) {
      throw Error("required browser with fetch support");
    }
    if ('function' !== typeof window.TextDecoder) {
      throw Error("required browser with TextDecoder support");
    }
    if ('function' !== typeof window.ImageBitmap) {
      throw Error("required browser with ImageBitmap support");
    }
  } catch(err) {
    if (err.message.substr(0, 22) === 'required browser with ') {
      $id('alert-feature').innerHTML = '<strong>' + err.message.substr(22).split(' ', 1)[0] + '</strong>';
    }
    throw err;
  }
  alert.parentElement.removeChild(alert);
}

// this function is called directly from wasm
export function now() {
  return performance.now();
}
