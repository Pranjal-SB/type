use std::path::Path;

use typ_core::HandlerId;
use typ_registry::Registry;

#[test]
fn unknown_extensions_fall_back_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("a.zzz")), HandlerId("editor"));
}

#[test]
fn files_without_an_extension_fall_back_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("Makefile")), HandlerId("editor"));
}

#[test]
fn known_text_extensions_route_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("main.rs")), HandlerId("editor"));
}

#[test]
fn registering_a_handler_overrides_the_fallback() {
    let mut r = Registry::with_builtins();
    r.register("png", HandlerId("image"));
    assert_eq!(r.handler_for(Path::new("logo.png")), HandlerId("image"));
}

#[test]
fn extension_matching_is_case_insensitive() {
    let mut r = Registry::with_builtins();
    r.register("png", HandlerId("image"));
    assert_eq!(r.handler_for(Path::new("LOGO.PNG")), HandlerId("image"));
}
