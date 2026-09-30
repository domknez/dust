//! Decodes the app icon PNGs to raw RGBA at build time (so the binary needs no PNG
//! decoder at runtime) and, on Windows, embeds dust.ico into the executable.

use std::fs::File;
use std::path::Path;

const ICONS: &[(&str, &str)] = &[
    ("assets/dust-icon-pack/png/dust-256.png", "icon-256.rgba"),
    ("assets/dust-icon-pack/png/dust-64.png", "icon-64.rgba"),
];

fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    for (src, dst) in ICONS {
        println!("cargo:rerun-if-changed={src}");
        let decoder = png::Decoder::new(File::open(src).unwrap_or_else(|e| panic!("{src}: {e}")));
        let mut reader = decoder.read_info().expect("png header");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("png data");
        assert_eq!(info.color_type, png::ColorType::Rgba, "{src}: expected RGBA");
        assert_eq!(info.bit_depth, png::BitDepth::Eight, "{src}: expected 8-bit");
        buf.truncate(info.buffer_size());
        std::fs::write(Path::new(&out).join(dst), &buf).expect("write icon");
    }

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/dust-icon-pack/dust.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/dust-icon-pack/dust.ico");
        res.compile().expect("embed Windows icon");
    }
}
