export function init() {
  var handle = this.handle;
  if (handle) {
    return handle;
  }
  else {
    return this.handle = import("./pkg").then(wasm => wasm.ZxSpectrumEmu);
  }
}
