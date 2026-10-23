use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut fonts: Vec<_> = fs::read_dir(root.join("fonts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("ttf" | "otf")
            )
        })
        .collect();
    fonts.sort();
    println!("cargo:rerun-if-changed=../fonts");
    println!("cargo:rerun-if-changed=../bgs");
    println!("cargo:rerun-if-changed=build.rs");
    // Compression is a build step; the application ships only the smaller bytes.
    let compressed: Vec<_> = std::thread::scope(|scope| {
        let jobs: Vec<_> = fonts
            .iter()
            .map(|path| {
                let out = &out;
                scope.spawn(move || {
                    let original = fs::read(path).unwrap();
                    let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                    let target = out.join(format!("{name}.br"));
                    let reusable = fs::read(&target).ok().filter(|bytes| {
                        let mut restored = Vec::new();
                        brotli::Decompressor::new(bytes.as_slice(), 4096)
                            .read_to_end(&mut restored)
                            .is_ok()
                            && restored == original
                    });
                    let bytes = reusable.unwrap_or_else(|| {
                        let mut bytes = Vec::new();
                        {
                            let mut compressor =
                                brotli::CompressorWriter::new(&mut bytes, 65536, 11, 22);
                            compressor.write_all(&original).unwrap();
                        }
                        let mut restored = Vec::new();
                        brotli::Decompressor::new(bytes.as_slice(), 4096)
                            .read_to_end(&mut restored)
                            .unwrap();
                        assert_eq!(
                            restored, original,
                            "Font compression must preserve every byte"
                        );
                        fs::write(&target, &bytes).unwrap();
                        bytes
                    });
                    (name, target, original.len(), bytes.len())
                })
            })
            .collect();
        jobs.into_iter().map(|job| job.join().unwrap()).collect()
    });
    let mut code = String::from("pub const FONT_NAMES: &[&str] = &[\n");
    for (name, ..) in &compressed {
        code += &format!("{name:?},\n");
    }
    code += "];\nfn embedded_font(name: &str) -> Option<(&'static [u8], usize)> {\nmatch name {\n";
    for (name, path, length, _) in &compressed {
        code += &format!("{name:?} => Some((include_bytes!({path:?}), {length})),\n");
    }
    code += "_ => None,\n}}\n";
    let mut backgrounds: Vec<_> = fs::read_dir(root.join("bgs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("png" | "jpg" | "jpeg" | "webp")
            )
        })
        .collect();
    backgrounds.sort();
    code += "pub const BACKGROUND_NAMES: &[&str] = &[\n";
    for path in &backgrounds {
        code += &format!("{:?},\n", path.file_name().unwrap().to_str().unwrap());
    }
    code += "];\nfn embedded_background(name: &str) -> Option<&'static [u8]> {\nmatch name {\n";
    for path in &backgrounds {
        code += &format!(
            "{:?} => Some(include_bytes!({:?})),\n",
            path.file_name().unwrap().to_str().unwrap(),
            path.canonicalize().unwrap()
        );
    }
    code += "_ => None,\n}}\n";
    fs::write(out.join("embedded_assets.rs"), code).unwrap();
    let raw: usize = compressed.iter().map(|item| item.2).sum();
    let packed: usize = compressed.iter().map(|item| item.3).sum();
    println!(
        "cargo:warning=Embedded {} complete fonts: {raw} -> {packed} bytes (lossless)",
        compressed.len()
    );
}
