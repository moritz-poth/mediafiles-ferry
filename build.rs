fn main() {
    let style = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") { "cupertino" } else { "fluent" };
    let config = slint_build::CompilerConfiguration::new()
        .with_style(style.into())
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).unwrap();
}
