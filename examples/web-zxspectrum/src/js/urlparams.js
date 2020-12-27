/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the index.js file.
*/
import { setupDevice } from "./utils";
/*
  #m={{model}}
  #i=0|1|2
  #b=0-6
  #j=K|F|S1|S2|C
  #ay=ABC,Fuse|Spec,Melodik,Fuller
  #k=2|3
  #tf=0
  #ti=0
  #ta=0
  #km=0
  #t=index
  #tap=...
  #sna=...
  #z80=...
  #json=...
  #scr=...
*/
 
const SimpleOptions = {
  m:   "model",
  i:   "interlace",
  b:   "border",
  j:   "joystick",
  k:   "keyboard",
  t:   "tapChunk"
};

const BooleanOptions = {
  tf: "fastTape",
  ti: "instantTape",
  ta: "audibleTape",
  km: "kempstonMouse"
};

export class UrlParameters {
  constructor() {
    const hash = location.hash;
    this.options = parseOptions(hash);
    this.scr = null;
    this.tap = null;
    this.snap = null;
    this.cache = hash;
  }

  hashChanged() {
    const hash = location.hash;
    if (hash != this.cache) {
      this.options = parseOptions(hash);
      this.cache = hash;
      return this.options;
    }
  }

  mergeAll(spectrum) {
    var tags = [];
    const options = this.options;
    const ay = options.ay || (options.ay = {});
    if (options.model == null) tags.push("models");
    if (options.interlace == null) tags.push("interlace");
    if (options.border == null) tags.push("borders");
    if (options.joystick == null) tags.push("joysticks");
    if (options.fastTape == null) tags.push("fast-tape");
    if (options.instantTape == null) tags.push("instant-tape");
    if (options.audibleTape == null) tags.push("audible-tape");
    if (options.kempstonMouse == null) tags.push("kempston-mouse");
    if (ay.melodik == null) tags.push("ay-melodik");
    if (ay.fuller == null) tags.push("ay-fuller-box");
    if (ay.amps == null) tags.push("ay-amps");
    if (ay.channels == null) tags.push("ay-channels");
    tags.forEach(tag => this.updateFrom(spectrum, tag));
    this.updateLocation();
  }

  updateAll(spectrum) {
    ["models", "interlace", "borders", "joysticks", "fast-tape", "instant-tape", "audible-tape", "ay-amps", "ay-channels"]
    .forEach(tag => this.updateFrom(spectrum, tag));
    this.updateLocation();
  }

  updateFrom(spectrum, tag) {
    const options = this.options;
    const ay = options.ay || (options.ay = {});
    switch (tag) {
      case "models":
        options.model = lastTag(spectrum.model);
        ["ay-melodik", "ay-fuller-box", "kempston-mouse"].forEach(t => this.updateFrom(spectrum, t));
        // fall to keyboard-issue
      case "keyboard-issue": options.keyboard = shortIssue(spectrum.keyboardIssue); break;
      case "interlace": options.interlace = spectrum.interlace; break;
      case "borders": options.border = shortBorderSize(spectrum.borderSize); break;
      case "joysticks": options.joystick = shortJoystick(spectrum.joystick); break;
      case "fast-tape": options.fastTape = spectrum.fastTape; break;
      case "instant-tape": options.instantTape = spectrum.instantTape; break;
      case "audible-tape": options.audibleTape = spectrum.audibleTape; break;
      case "ay-melodik": ay.melodik = spectrum.hasDevice("Melodik"); break;
      case "ay-fuller-box": ay.fuller = spectrum.hasDevice("Fuller Box"); break;
      case "kempston-mouse": options.kempstonMouse = spectrum.hasDevice("Kempston Mouse"); break;
      case "ay-amps": ay.amps = spectrum.ayAmps.toLowerCase(); break;
      case "ay-channels": ay.channels = spectrum.ayChannels; break;
      case "files": this.removeTap(); break;
      case "reset-hard":
      case "reset-soft":
      case "reset-power":
      case "trigger-nmi": this.removeSnap(); break;
      case "tap-eject":
        this.removeTap();
        // fall below
      case "tap-chunks": {
        let [index, _] = spectrum.tapeProgress();
        if (index < 0) {
          delete options.tapChunk;
        }
        else {
          options.tapChunk = "" + index;
        }
        break;
      }
      default: return false;
    }
    return true;
  }

  updateLocation() {
    const hash = optionsToHash(this.options);
    this.cache = hash;
    location.hash = hash;
  }

  applyTo(spectrum) {
    const {
      model,
      interlace,
      border,
      joystick,
      keyboard,
      fastTape,
      instantTape,
      audibleTape,
      kempstonMouse,
      ay,
      tapChunk,
      // scr,
      // tap,
      // snap
    } = this.options;

    if (model) tryCall(() => spectrum.selectModel(model));
    tryCall(() => spectrum.interlace = interlace|0);
    tryCall(() => spectrum.selectBorderSize(border || "full"));
    tryCall(() => spectrum.selectJoystick(parseJoystick(joystick || "")));
    if (spectrum.keyboardIssue.startsWith("Issue")) {
      tryCall(() => spectrum.keyboardIssue = "Issue " + (keyboard || "3"));
    }
    spectrum.fastTape = fastTape == null || fastTape;
    spectrum.instantTape = instantTape == null || instantTape;
    spectrum.audibleTape = audibleTape == null || audibleTape;
    setupDevice(spectrum, "Kempston Mouse", kempstonMouse);
    let { amps, melodik, fuller, channels } = ay || {};
    setupDevice(spectrum, "Melodik", melodik);
    setupDevice(spectrum, "Fuller Box", fuller);
    tryCall(() => spectrum.ayAmps = amps || "Spec");
    tryCall(() => spectrum.ayChannels = (channels || "ACB").toUpperCase());
    if (tapChunk != null) tryCall(() => spectrum.selectTapeChunk(tapChunk));
  }

  removeTap() {
    delete this.options.tap;
    this.tap = null;
  }

  removeSnap() {
    delete this.options.snap;
    this.snap = null;
    delete this.options.scr;
    this.scr = null;
  }

  modifiedTap() {
    const tap = this.options.tap;
    var res = tap != this.tap;
    this.tap = tap || null;
    return res;
  }

  modifiedSnap() {
    var res = true;
    const snap = this.options.snap;
    if (snap) {
      if (this.snap) {
        res = snap.type !== this.snap.type || snap.url !== this.snap.url;
      }
      if (res) {
        this.snap = {type: snap.type, url: snap.url};
      }
    }
    else {
      res = !!this.snap;
      this.snap = null;
    }
    return res;
  }

  modifiedScr() {
    const scr = this.options.scr;
    var res = scr != this.scr;
    this.scr = scr || null;
    return res && !!scr;
  }
}

function parseOptions(hashstr) {
  var tap, options = {};
  for (let item of hashstr.split("#")) {
    let eqIndex = item.indexOf("=");
    let key = item.substr(0, eqIndex).toLowerCase();
    let value = item.substr(eqIndex + 1);
    switch (key) {
      case "ay":
        options.ay = parseAyOptions(value.split(","));
        break;
      case "scr":
        options.scr = value;
        break;
      case "tap":
        options.tap || (options.tap = tap = []);
        tap.push(value);
        break;
      case "sna":
      case "z80":
      case "json":
        options.snap = {type: key, url: value};
        break;
      default:
        let name = SimpleOptions[key];
        if (name) {
          options[name] = value;
        }
        else {
          name = BooleanOptions[key];
          if (name) {
            options[name] = value != "" && value != "0";
          }
        }
        break;
    }
  }
  return options;
}

function parseAyOptions(tags) {
  var options = {};
  for (let tag of tags) {
    switch (tag = tag.toLowerCase()) {
      case "fuse":
      case "spec":
        options.amps = tag;
        break;
      case "melodik":
      case "fuller":
        options[tag] = true;
        break;
      case "mono":
        options.channels = tag;
      default:
        if (/^[abc]{3}$/.test(tag)) {
          options.channels = tag.toUpperCase();
        }
    }
  }
  return options;
}

function optionsToHash(options) {
  var hash = "";
  for (let key in SimpleOptions) {
    let value = options[SimpleOptions[key]];
    if (value) {
      hash += `#${key}=${value}`;
    }
  }

  for (let key in BooleanOptions) {
    let value = options[BooleanOptions[key]];
    if (value != null) {
      hash += `#${key}=${value ? "1" : "0"}`;
    }
  }

  hash += ayOptionsToHash(options.ay || {});

  let tap = options.tap;
  if (tap) {
    for (let t of tap) {
      hash += `#tap=${t}`;
    }
  }

  let snap = options.snap;
  if (snap) {
    hash += `#${snap.type}=${snap.url}`;
  }

  let scr = options.scr;
  if (scr) {
    hash += `#scr=${scr}`;
  }

  return hash;
}

function ayOptionsToHash(options) {
  var tags = ["amps", "channels", "melodik", "fuller"]
    .filter(tag => !!options[tag])
    .map(tag => {
      let value = options[tag];
      if ("string" === typeof value) {
        return value;
      }
      else {
        return tag;
      }
    })
    .join(",");
  return tags ? `#ay=${tags}` : "";
}

function tryCall(callback) {
  try {
    callback();
  }
  catch(e) {
    alert(e);
  }
}

function parseJoystick(name) {
  switch (name.toLowerCase()) {
    case "k": case "kempston": return 0;
    case "f": case "fuller": return 1;
    case "sr": case "sinclair right": return 2;
    case "sl": case "sinclair left": return 3;
    case "c": case "cursor": case "protek": case "agf": return 4;
    default: {
      let index = parseInt(name);
      if (index >= 0 && index <= 4) {
        return index;
      }
      return -1;
    }
  }
}

function lastTag(name) {
  return name.substr(name.lastIndexOf(" ") + 1);
}

const BorderSizes = {
  full: "6", large: "5", medium: "4", small: "3", tiny: "2", minimal: "1", none: "0"
};

function shortBorderSize(name) {
  return BorderSizes[name] || "6";
}

const JoystickNames = {
  Kempston: "k",
  Fuller: "f",
  "Sinclair Right": "sr",
  "Sinclair Left": "sl",
  Cursor: "c"
};

function shortJoystick(name) {
  return JoystickNames[name] || "";
}

function shortIssue(name) {
  if (name.startsWith("Issue")) {
    return lastTag(name);
  }
  else {
    return "";
  }
}
