/*
    web-ay-player: Web ZX Spectrum AY file format audio player example.
    Copyright (C) 2020-2022  Rafal Michalski

    web-ay-player is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    web-ay-player is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
import("./pkg")
  .then(rust_module => {
    let ayPlayer = null;
    let ayFileList = [];

    function clearNode(node) {
      while (node.firstChild) {
        node.removeChild(node.firstChild);
      }
    }

    function play(file, filename, songIndex) {
      if (ayPlayer !== null) {
        ayPlayer.free();
      }
      songIndex >>>= 0;
      ayPlayer = new rust_module.AyPlayerHandle(0.125); // a single buffer duration
      ayPlayer.setClocking(JSON.stringify(clockingSelect.value));
      ayPlayer.setAmps(JSON.stringify(ampsSelect.value));
      ayPlayer.setChannels(JSON.stringify(channelsSelect.value));
      ayPlayer.setGain(gainControl.value / 100);
      ayPlayer.load(file).then(info => {
        info.file = filename;
        showInfo(info, songIndex);
        ayPlayer.play(songIndex);
      }, e => alert(e));
    }

    const gainElem = document.getElementById("gainControl");
    const filesInput = document.getElementById("files");
    const pauseButton = document.getElementById("pause");
    const replayButton = document.getElementById("replay");
    const channelsSelect = document.getElementById("channels");
    const clockingSelect = document.getElementById("clocks");
    const ejectButton = document.getElementById("eject");
    const musicSelect = document.getElementById("music");
    const ampsSelect = document.getElementById("amps");
    const songsElem = document.getElementById("songs");

    function showInfo(info, songIndex) {
      for (let label of ["file", "author", "misc"]) {
        let elem = document.getElementById("info-" + label);
        clearNode(elem);
        elem.appendChild(document.createTextNode(String(info[label] || "")));
      }
      populateSongs(info.songs, songIndex)
    }

    function populateSongs(songs, songIndex) {
      clearNode(songsElem);
      if (Array.isArray(songs)) {
        for (let i = 0, numSongs = songs.length; i < numSongs; i++) {
          let option = document.createElement("option");
          option.value = i;
          option.text = "(" + (i + 1) + ") " + songs[i].name;
          if (i == songIndex) {
            option.selected = true;
          }
          songsElem.appendChild(option);
        }
      }
    }

    function playSelectedSong() {
      let index = musicSelect.selectedIndex;
      let option = musicSelect.options[index];
      let file = ayFileList[index];
      if (option && file) {
        play(file, option.value, songsElem.value);
      }
    }

    songsElem.addEventListener("change", playSelectedSong, false);

    musicSelect.addEventListener("change", event => {
      let option = musicSelect.options[musicSelect.selectedIndex];
      play(ayFileList[musicSelect.selectedIndex], option.value);
    }, false);

    filesInput.addEventListener("change", event => {
      const files = filesInput.files;
      let wasEmpty = musicSelect.options.length == 0;
      for (let i = 0, numFiles = files.length; i < numFiles; i++) {
        let file = files[i];
        let option = document.createElement("option");
        option.value = file.name;
        option.text = file.name;
        ayFileList.push(file);
        musicSelect.appendChild(option);
      }
      if (wasEmpty && files.length != 0) {
        let option = musicSelect.options[0];
        option.selected = true;
        play(ayFileList[0], option.value);
      }
    }, false);

    pauseButton.addEventListener("click", event => {
      if (ayPlayer !== null) {
        ayPlayer.togglePause().then(_paused => {})
      }
      else {
        playSelectedSong();
      }
    }, false);

    replayButton.addEventListener("click", playSelectedSong, false);

    ejectButton.addEventListener("click", event => {
      let index = musicSelect.selectedIndex;
      if (index >= 0) {
        let option = musicSelect.options[index];
        if (option) {
          musicSelect.removeChild(option);
          ayFileList.splice(index, 1);

          showInfo({});

          if (ayPlayer !== null) {
            ayPlayer.free();
            ayPlayer = null;
          }
        }
      }
    }, false);

    gainElem.addEventListener("input", event => {
      ayPlayer && ayPlayer.setGain(parseFloat(event.target.value/100));
    }, false);

    channelsSelect.addEventListener("change", event => {
      ayPlayer && ayPlayer.setChannels(JSON.stringify(event.target.value));
    }, false);

    ampsSelect.addEventListener("change", event => {
      ayPlayer && ayPlayer.setAmps(JSON.stringify(event.target.value));
    }, false);

    clockingSelect.addEventListener("change", event => {
      ayPlayer && ayPlayer.setClocking(JSON.stringify(event.target.value));
    }, false);
  })
.catch(console.error);