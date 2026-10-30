//! Compile-time product identity generated from the independent GPUI metadata.
//! Stable Tauri metadata is intentionally not an input.
include!(concat!(env!("OUT_DIR"), "/product_constants.rs"));
