/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the index.js file.
*/
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
        if (typeof this.onchange === "function") {
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
        case ",": return [start, start + end];
        case ":": return [start, end];
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
        case ",": case "=": return [start, val];
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

export function $off(el, ev, cb) {
  el.removeEventListener(ev, cb, false);
}

export const Directions = {
  UP: 1,
  RIGHT: 2,
  DOWN: 4,
  LEFT: 8
};

const { abs } = Math;

export function onSwipe(element, directions, threshold, handler) {
  var pendingTouch = null;
  const { UP, RIGHT, DOWN, LEFT } = Directions;

  $on(element, "touchstart", handleStart);
  $on(element, "touchcancel", handleCancel);
  $on(element, "touchmove", handleMove);
  $on(element, "touchend", handleEnd);

  function handleStart(ev) {
    var touches = ev.changedTouches;
    ev.preventDefault();
    if (touches.length === 1 && pendingTouch == null) {
      let touch = touches[0];
      pendingTouch = {id: touch.identifier, x: touch.screenX, y: touch.screenY};
    }
    else {
      handleCancel(ev);
    }
  }

  function handleCancel(ev) {
    ev.preventDefault();
    pendingTouch = null;
  }

  function handleMove(ev) {
    var touches = ev.changedTouches;
    if (pendingTouch != null && touches.length === 1 && touches[0].identifier === pendingTouch.id) {
      let { screenX, screenY } = touches[0],
          dx = screenX - pendingTouch.x,
          dy = screenY - pendingTouch.y,
          mx = abs(dx),
          my = abs(dy);

      if (mx > threshold || my > threshold) {
        let dir = 0;
        mx = mx * 2;
        my = my * 2;
        if (-dy > mx) {
          dir = UP;
        }
        else if (dx > my) {
          dir = RIGHT;
        }
        else if (dy > mx) {
          dir = DOWN;
        }
        else if (-dx > my) {
          dir = LEFT;
        }

        handleCancel(ev);

        if ((dir & directions) !== 0) {
          handler(dir);
        }
      }
      else {
        ev.preventDefault();
      }
    }
    else {
      handleCancel(ev);
    }
  }

  function handleEnd(ev) {
    handleMove(ev);
    pendingTouch = null;
  }
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
const STORAGE_PREFIX = 'SPECTRUSTY:';

export function stateGuard(spectrum, urlparams) {
  const storage = window.localStorage,
        GRACE_TIME = 1000;
  var lastTimeStamp = 0;
  return function stateSave(event) {
    var timeStamp = event.timeStamp;
    if (storage != null && timeStamp > lastTimeStamp + GRACE_TIME) {
      lastTimeStamp = timeStamp;
      const ident = urlparams.cache,
            key = window.location.pathname,
            keyId = STORAGE_PREFIX + 'i' + key,
            keyVal = STORAGE_PREFIX + 'v' + key;

      if (FRESH_TEST.test(ident)) {
        storage.removeItem(keyId);
        storage.removeItem(keyVal);
      }
      else {
        let value = spectrum.toJSON();
        storage.setItem(keyId, ident);
        storage.setItem(keyVal, value);
      }
    }
  }
}

export function restoreState(spectrum, urlparams) {
  const storage = window.localStorage,
        ident = urlparams.cache,
        key = window.location.pathname,
        keyId = STORAGE_PREFIX + 'i' + key,
        keyVal = STORAGE_PREFIX + 'v' + key;

  if (storage != null && !FRESH_TEST.test(ident) && ident === storage.getItem(keyId)) {
    let json = storage.getItem(keyVal);
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

export function splash(canvas) {
    const { width, height } = canvas;
    const ctx = canvas.getContext("2d");
    const w8 = width / 8, h8 = height / 8;
    for (let [i, color] of ["#D80000", "#D8D800", "#00D800", "#00D8D8"].entries()) {
      ctx.beginPath();
      ctx.moveTo((4+i)*w8, height);
      ctx.lineTo(width, (4+i)*h8);
      ctx.lineTo(width, (5+i)*h8);
      ctx.lineTo((5+i)*w8, height);
      ctx.fillStyle = color;
      ctx.strokeStyle = color;
      ctx.fill();
      ctx.stroke();
    }
    ctx.font = "48px sans-serif";
    ctx.textBaseline = "middle";
    ctx.textAlign = "end";
    ctx.fillStyle = "#D8D8D8";
    ctx.fillText("S P E C T ", width/2, height/2);
    ctx.textAlign = "start";
    ctx.fillStyle = "#880000";
    ctx.fillText("R U S T Y", width/2, height/2);
}

export function checkBrowserCapacity() {
  var alert = $id("alert");
  try {
    /* check edge features */
    if ("undefined" === typeof window.WebAssembly) {
      throw Error("required browser with WebAssembly support");
    }
    if ("function" !== typeof window.requestAnimationFrame) {
      throw Error("required browser with requestAnimationFrame support");
    }
    if ("function" !== typeof window.fetch) {
      throw Error("required browser with fetch support");
    }
    if ("function" !== typeof window.TextDecoder) {
      throw Error("required browser with TextDecoder support");
    }
    if ("function" !== typeof window.ImageBitmap) {
      throw Error("required browser with ImageBitmap support");
    }
  } catch(err) {
    if (err.message.substr(0, 22) === "required browser with ") {
      $id("alert-feature").innerHTML = "<strong>" + err.message.substr(22).split(" ", 1)[0] + "</strong>";
    }
    throw err;
  }
  alert.parentElement.removeChild(alert);
}

// this function is called directly from wasm
export function now() {
  return performance.now();
}
