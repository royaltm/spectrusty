/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the index.js file.
*/
import { $on } from "./utils";

const ROWKEYS = [
//  1  2  3  4  5  6  7  8  9  0
  [35,27,19,11, 3, 4,12,20,28,36],
//  Q  W  E  R  T  Y  U  I  O  P
  [34,26,18,10, 2, 5,13,21,29,37],
//  A  S  D  F  G  H  J  K  L EN
  [33,25,17, 9, 1, 6,14,22,30,38],
// CS  Z  X  C  V  B  N  M SS BR
  [32,24,16, 8, 0, 7,15,23,31,39]
];
const SSKEY = 31;
const CSKEY = 32;
const NKEYS = ROWKEYS.reduce((sum, row) => sum + row.length, 0);
const KEYAREA = new Array(NKEYS);

ROWKEYS.forEach((row, n) => row.forEach((key, k) => {
  KEYAREA[key] = keyRect(n, k);
}));

export class SpectrumKeyboard {
  constructor(canvas, imgsrc) {
    this.canvas = canvas;
    this.state = new Array(NKEYS).fill(false);
    this.lastMouseKey = null;
    this.pendingTouches = new Map();
    canvas.width = 704;
    canvas.height = 281;

    this.ctx = canvas.getContext("2d", {alpha: false, desynchronized: true});

    const keyboard = new Image(canvas.width, canvas.height);
    keyboard.onload = () => {
      canvas.width = keyboard.naturalWidth;
      canvas.height = keyboard.naturalHeight;
      this.keyboard = keyboard;
      this.redraw();
    };
    keyboard.src = imgsrc;
  }

  bind(onkey) {
    if (typeof onkey === "function") {
      if (typeof this.onkey !== "function") {
        const element = this.canvas;
        $on(element, "mousedown", ev => mouseHandler.call(this, ev, true));
        $on(element, "mouseup", ev => mouseHandler.call(this, ev, false));
        $on(element, "mouseout", ev => mouseHandler.call(this, ev, false));
        $on(element, "touchstart", ev => touchDown.call(this, ev));
        $on(element, "touchmove", ev => touchDown.call(this, ev));
        $on(element, "touchend", ev => touchEnd.call(this, ev));
        $on(element, "touchcancel", ev => touchEnd.call(this, ev));
      }
      this.onkey = onkey;
    }
  }

  update(keymap) {
    var state = this.state,
        lomap = keymap>>>0;

    for(var key = 0, limit = 32;;limit = NKEYS) {
      for (;key < limit; ++key) {
        state[key] = lomap & 1 === 1;
        lomap >>>=1;
      }
      if (key >= NKEYS) break;
      lomap = (keymap / 0x100000000)>>>0;
    }

    return this;
  }

  redraw() {
    const keyboard = this.keyboard;
    if (!keyboard) return;
    const state = this.state;
    const ctx = this.ctx;
    ctx.globalAlpha = 1;
    ctx.drawImage(keyboard, 0, 0);
    ctx.globalAlpha = 0.5;
    ctx.fillStyle = "blue";
    for (var key = 0; key < NKEYS; ++key) {
      if (state[key]) ctx.fill(KEYAREA[key]);
    }
  }
}

function mouseHandler(event, pressed) {
  var key;

  const updateState = (key, pressed) => {
    this.state[key] = pressed;
    this.onkey(key, pressed);
  };

  if (pressed) {
    // sanity check
    if (this.lastMouseKey != null) {
      updateState(this.lastMouseKey, false);
      this.lastMouseKey = null;
    }
    key = keyFromOffsetCoords(event.offsetX, event.offsetY, this.canvas);
  }
  else {
    key = this.lastMouseKey;
  }

  if (key == null) return;

  // make CS and SS keys sticky...
  // ...unless the other is pressed
  const isSticky = (key === CSKEY && !this.state[SSKEY]) || (key === SSKEY && !this.state[CSKEY]);
  if (pressed || !isSticky) {
    if (pressed && this.state[key]) {
      pressed = false;
    }
    this.lastMouseKey = pressed && !isSticky ? key : null;
    updateState(key, pressed);
    if (!pressed) {
      if (key !== CSKEY && this.state[CSKEY]) updateState(CSKEY, false);
      if (key !== SSKEY && this.state[SSKEY]) updateState(SSKEY, false);
    }
    this.redraw();
  }
}

function touchDown(event) {
  const canvas = this.canvas,
        touches = event.changedTouches,
        pendingTouches = this.pendingTouches;

  event.preventDefault();

  var changed = false;

  for (let i = 0; i < touches.length; ++i) {
    let touch = touches[i],
        id = touch.identifier,
        key = keyFromOffsetCoords(touch.clientX - canvas.offsetLeft,
                                  touch.clientY - canvas.offsetTop,
                                  canvas),
        prev = pendingTouches.get(id);

    if (key == prev) continue;

    if (key != null) {
      pendingTouches.set(id, key);
      this.state[key] = true;
      this.onkey(key, true);
    }
    else {
      pendingTouches.delete(id);
    }

    if (prev != null) {
      this.state[prev] = false;
      this.onkey(prev, false);
    }
    changed = true;
  }

  if (changed) this.redraw();
}

function touchEnd(event) {
  const touches = event.changedTouches,
        pendingTouches = this.pendingTouches;

  event.preventDefault();

  for (let i = 0; i < touches.length; ++i) {
    let touch = touches[i],
        id = touch.identifier,
        key = pendingTouches.get(id);

    if (key != null) {
      pendingTouches.delete(id);
      this.state[key] = false;
      this.onkey(key, false);
    }
  }

  this.redraw();
}

function keyFromOffsetCoords(x, y, canvas) {
  const { width, height, clientWidth, clientHeight } = canvas,
        rx = width / clientWidth,
        ry = height / clientHeight;

  if (rx > ry) {
    x = x * rx;
    y = y * rx - (rx * clientHeight - height) / 2.0;
  }
  else {
    x = x * ry - (ry * clientWidth - width) / 2.0;
    y = y * ry;
  }

  return keyFromCoords(x, y);
}

function keyFromCoords(x, y) {
  if (y > 0) {
    if (y < 74) {
      if (x >= 1 && x < 651) {
        return ROWKEYS[0][((x - 1) / 65)|0];
      }
    }
    else if (y < 135) {
      if (x >= 35 && x < 685) {
        return ROWKEYS[1][((x - 35) / 65)|0];
      }
    }
    else if (y < 203) {
      if (x >= 52 && x < 702) {
        return ROWKEYS[2][((x - 52) / 65)|0];
      }
    }
    else if (y < 267 && x > 0) {
      if (x < 85) {
        return ROWKEYS[3][0];
      }
      else if (x < 605) {
        return ROWKEYS[3][1 + (((x - 85) / 65)|0)];
      }
      else if (x <= 700) {
        return ROWKEYS[3][9];
      }
    }
  }
}

function keyRect(row, rkey) {
  var x, y, w = 47;
  const rect = new Path2D();

  switch(row) {
    case 0:
      y = 27; x = 12 + rkey * 65; break;
    case 1:
      y = 92; x = 44 + rkey * 65; break;
    case 2:
      y = 158; x = 60 + rkey * 65; break;
    default:
      y = 223;
      switch(rkey) {
        case 0:
          x = 12; w = 63; break;
        case 9:
          x = 614; w = 78; break;
        default:
          x = 94 + (rkey - 1) * 65;
      }
  }

  rect.rect(x, y, w, 32);
  return rect;
}
