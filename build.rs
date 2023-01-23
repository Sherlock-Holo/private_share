use std::env;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use brotli::enc;
use brotli::enc::BrotliEncoderParams;
use rayon::prelude::*;
use walkdir::WalkDir;

fn main() {
    if cfg!(feature = "build-web") {
        println!("cargo:rerun-if-changed=./ui/lib");

        clean_web();
        build_web();
        let paths = collect_web_files();
        compress_web(paths)
    }
}

fn clean_web() {
    if !env::var_os("CLEAN_WEB")
        .map(|value| value == OsStr::new("1"))
        .unwrap_or(false)
    {
        return;
    }

    let output = Command::new("flutter")
        .arg("clean")
        .current_dir("./ui")
        .output()
        .unwrap_or_else(|err| panic!("start flutter clean failed: {err}"));

    if output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    panic!("clean web failed, stdout {stdout}\nstderr{stderr}");
}

fn build_web() {
    let output = Command::new("flutter")
        .args("build web --verbose --base-href /ui/ --dart-define FLUTTER_WEB_CANVASKIT_URL=https://npm.elemecdn.com/canvaskit-wasm@0.35.0/bin/".split(' '))
        .current_dir("./ui")
        .output()
        .unwrap_or_else(|err| panic!("start flutter build web failed: {err}"));

    if output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    panic!("build web failed, stdout {stdout}\nstderr{stderr}");
}

fn collect_web_files() -> Vec<PathBuf> {
    WalkDir::new("./ui/build/web")
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.unwrap_or_else(|err| panic!("walk dir failed: {err}"));

            if !entry.file_type().is_file() {
                return None;
            }

            let filename = entry.path().file_name()?;
            if filename == OsStr::new(".last_build_id")
                || filename == OsStr::new("version.json")
                || filename == OsStr::new("NOTICES")
                || filename == OsStr::new("canvaskit.wasm")
            {
                return None;
            }
            if Path::new(filename).extension() == Some(OsStr::new("br")) {
                return None;
            }

            Some(entry.into_path())
        })
        .collect()
}

fn compress_web(paths: Vec<PathBuf>) {
    paths
        .into_par_iter()
        .try_for_each(|path| {
            let mut br_path = path.as_os_str().to_os_string();
            br_path.push(".br");
            let br_path = PathBuf::from(br_path);

            let mut file = File::open(&path)?;
            let mut br_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(br_path)?;

            enc::BrotliCompress(&mut file, &mut br_file, &BrotliEncoderParams::default())?;

            eprintln!("compress {path:?} done");

            Ok::<_, Error>(())
        })
        .unwrap_or_else(|err| panic!("compress web failed: {err}"))
}
