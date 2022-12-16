use std::env;
use std::process::Command;
use std::collections::VecDeque;

fn main() {
  let mut args: VecDeque<_> = env::args().skip(1).collect();
  let cmd = args.pop_front().expect("a rustc command");
  // eprintln!("=============================");
  if &args[0] == "--crate-name" {
    // eprintln!("CRATE: {}", &args[1]);
    match args[1].as_str() {
      "hyper"|"reqwest"|"idna"|"ipnet"|"mime"|"native_tls"|"winreg"|
      "url"|"tokio_util"|"tokio_native_tls"|"httpdate"|
      "encoding_rs" => {},
      "boot"|"synth"|"video"|"video128"|"video_plus"|"z80emu"|
      "spectrusty"|"spectrusty_core"|"spectrusty_formats"|
      "spectrusty_peripherals"|"spectrusty_utils"|
      "zxspectrum_common"|"web_zxspectrum"|"sdl2_zxspectrum" => {
        args.push_back("-Zmir-opt-level=4".into());
        args.push_back("-Zinline-mir=yes".into());
        args.push_back("-Zinline-mir-threshold=500".into());
        args.push_back("-Zinline-mir-hint-threshold=1000".into());
        args.push_back(format!("-Zprint-fuel={}", &args[1]));
      }
      _ => {
        args.push_back("-Zmir-opt-level=4".into());
        args.push_back("-Zinline-mir=yes".into());
        args.push_back(format!("-Zprint-fuel={}", &args[1]));
      }
    }
  }
  // eprintln!("{:?}", args);
  // eprintln!("=============================");
  Command::new(cmd)
            .args(args)
            .spawn()
            .expect("failed to execute process");
}
