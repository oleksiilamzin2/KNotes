use knotes_session::embedder_files;

const MODEL: &str = "some-model.gguf";

#[test]
fn the_embedder_and_its_model_are_found_inside_the_bundle() {
    let app = tempfile::tempdir().unwrap();
    let (macos, resources) = (app.path().join("Knotes.app/Contents/MacOS"), app.path().join("Knotes.app/Contents/Resources"));
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(&resources).unwrap();
    let exe = macos.join("knotes");
    assert_eq!(embedder_files(&exe, MODEL), None, "no helper beside the app");
    std::fs::write(macos.join("knotes-embed"), "").unwrap();
    std::fs::write(resources.join(MODEL), "").unwrap();
    assert_eq!(embedder_files(&exe, MODEL), Some((macos.join("knotes-embed"), resources.join(MODEL))));
}
