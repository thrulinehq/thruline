use super::*;
use std::os::unix::fs::symlink;
const KEY: &str = "d000000000000000000000000000000000000000000000000000000000000000";
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("internal-source/ui");
    let target = temp.path().join("external-target/full-commit");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(source.join("package.json"), "{\"name\":\"fixture\"}").unwrap();
    std::fs::write(source.join("package-lock.json"), "{\"lockfileVersion\":3}").unwrap();
    (
        temp,
        source.canonicalize().unwrap(),
        target.canonicalize().unwrap(),
    )
}
#[test]
fn manifests_stay_internal_and_only_links_enter_generated_workspace() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    assert!(workspace.root.starts_with(target.join("ui-build")));
    for name in ["package.json", "package-lock.json"] {
        assert!(workspace.root.join(name).is_symlink());
        assert_eq!(
            std::fs::read_link(workspace.root.join(name)).unwrap(),
            source.join(name)
        );
        assert!(std::fs::symlink_metadata(source.join(name))
            .unwrap()
            .is_file());
    }
}
#[test]
fn internal_dependency_link_only_targets_the_owned_generated_namespace() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    assert_eq!(
        std::fs::read_link(source.join("node_modules")).unwrap(),
        workspace.root.join("node_modules")
    );
    assert_eq!(
        std::fs::read_link(source.join("dist")).unwrap(),
        workspace.dist
    );
    assert!(workspace.dist.is_dir());
    workspace.verify().unwrap();
}
#[test]
fn npm_cache_logs_and_tmp_are_external_and_cwd_can_remain_internal() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    let mut command = Command::new("npm");
    command.current_dir(&source);
    workspace.configure(&mut command);
    let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
    assert_eq!(
        env.get(std::ffi::OsStr::new("npm_config_cache"))
            .copied()
            .flatten(),
        Some(workspace.npm_cache.as_os_str())
    );
    assert_eq!(
        env.get(std::ffi::OsStr::new("TMPDIR")).copied().flatten(),
        Some(workspace.temporary.as_os_str())
    );
    assert_eq!(command.get_current_dir(), Some(source.as_path()));
}
#[test]
fn npm_configuration_uses_distinct_pinned_empty_generated_files() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    let mut command = Command::new("npm");
    workspace.configure(&mut command);
    let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
    let user = PathBuf::from(env[OsStr::new("npm_config_userconfig")].unwrap());
    let global = PathBuf::from(env[OsStr::new("npm_config_globalconfig")].unwrap());
    assert_ne!(
        user, global,
        "npm refuses loading one path as both config scopes"
    );
    for path in [&user, &global] {
        assert!(path.starts_with(&workspace.root));
        assert!(std::fs::symlink_metadata(path).unwrap().is_file());
        assert!(std::fs::read(path).unwrap().is_empty());
    }
    std::fs::write(global, "changed config").unwrap();
    assert!(workspace.verify().is_err());
}
#[test]
fn redirecting_a_staging_link_invalidates_the_workspace() {
    let (temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    std::fs::remove_file(source.join("node_modules")).unwrap();
    symlink(temp.path(), source.join("node_modules")).unwrap();
    assert!(workspace.verify().is_err());
}
#[test]
fn tracked_dist_bytes_remain_internal_while_the_build_path_routes_external() {
    let (_temp, source, target) = fixture();
    std::fs::create_dir(source.join("dist")).unwrap();
    std::fs::write(source.join("dist/index.html"), "tracked exact source").unwrap();
    let mut tracked = VerifiedFile::read(&source.join("dist/index.html")).unwrap();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    let preserved = std::fs::read_dir(source.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".thruline-c1-preserved-ui-dist-")
        })
        .expect("exact source dist preserved internally");
    tracked.path = preserved.join("index.html");
    assert_eq!(
        std::fs::read_link(source.join("dist")).unwrap(),
        workspace.dist
    );
    assert!(tracked.remains_verified());
    assert_ne!(workspace.dist, source.join("dist"));
    workspace.verify().unwrap();
    std::fs::write(&tracked.path, "changed source dist").unwrap();
    assert!(workspace.verify().is_err());
}
#[test]
fn a_symlink_at_the_tracked_internal_dist_path_is_refused() {
    let (temp, source, target) = fixture();
    symlink(temp.path(), source.join("dist")).unwrap();
    assert!(UiWorkspace::prepare(&target, &source, KEY).is_err());
}
#[test]
fn replacing_the_external_workspace_inode_invalidates_it() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    std::fs::rename(&workspace.root, workspace.root.with_extension("retired")).unwrap();
    std::fs::create_dir(&workspace.root).unwrap();
    assert!(workspace.verify().is_err());
}
#[test]
fn missing_manifest_or_manifest_source_symlink_is_refused() {
    let (temp, source, target) = fixture();
    std::fs::remove_file(source.join("package-lock.json")).unwrap();
    assert!(UiWorkspace::prepare(&target, &source, KEY).is_err());
    symlink(temp.path().join("absent"), source.join("package-lock.json")).unwrap();
    assert!(UiWorkspace::prepare(&target, &source, KEY).is_err());
}
#[test]
fn symlinked_output_parent_is_never_followed() {
    let (temp, source, target) = fixture();
    symlink(temp.path(), target.join("ui-build")).unwrap();
    assert!(UiWorkspace::prepare(&target, &source, KEY).is_err());
    assert!(!temp.path().join(KEY).exists());
}
#[test]
fn changing_internal_source_manifest_invalidates_the_workspace() {
    let (_temp, source, target) = fixture();
    let workspace = UiWorkspace::prepare(&target, &source, KEY).unwrap();
    std::fs::write(source.join("package.json"), "changed").unwrap();
    assert!(workspace.verify().is_err());
}
#[test]
fn artifact_walk_names_regular_files_and_rejects_a_nested_symlink() {
    let (temp, _source, _target) = fixture();
    let dist = temp.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(dist.join("index.html"), "regular").unwrap();
    assert_eq!(
        generated_files(&dist).unwrap(),
        vec![dist.join("index.html")]
    );
    symlink(temp.path(), dist.join("alias")).unwrap();
    assert!(generated_files(&dist).is_err());
}
#[test]
fn artifact_walk_rejects_a_symlinked_dist_root() {
    let (temp, _source, _target) = fixture();
    let link = temp.path().join("dist");
    symlink(temp.path(), &link).unwrap();
    assert!(generated_files(&link).is_err());
}
#[test]
fn missing_external_root_is_not_recreated() {
    let (temp, source, _target) = fixture();
    let missing = temp.path().join("disconnected-target");
    assert!(UiWorkspace::prepare(&missing, &source, KEY).is_err());
    assert!(!missing.exists());
}
#[test]
fn package_cache_copy_rejects_a_symlink_and_stops_when_its_guard_fails() {
    let (temp, _source, target) = fixture();
    let cache = temp.path().join("package-cache");
    std::fs::create_dir(&cache).unwrap();
    std::fs::write(cache.join("tarball"), "synthetic-package").unwrap();
    let output = DirectoryPin::open(&target)
        .unwrap()
        .child(OsStr::new("copied"))
        .unwrap();
    let mut counts = (0, 0);
    copy_cache(&cache, &output, &mut counts, &mut || Ok(())).unwrap();
    assert_eq!(
        std::fs::read(output.path().join("tarball")).unwrap(),
        b"synthetic-package"
    );
    symlink(temp.path(), cache.join("forbidden-link")).unwrap();
    let output2 = DirectoryPin::open(&target)
        .unwrap()
        .child(OsStr::new("rejected"))
        .unwrap();
    assert!(copy_cache(&cache, &output2, &mut (0, 0), &mut || Ok(())).is_err());
    let output3 = DirectoryPin::open(&target)
        .unwrap()
        .child(OsStr::new("stopped"))
        .unwrap();
    assert!(copy_cache(&cache, &output3, &mut (0, 0), &mut || Err(
        "target disappeared"
    ))
    .is_err());
    assert_eq!(std::fs::read_dir(output3.path()).unwrap().count(), 0);
}

#[test]
#[ignore = "explicit native npm fixture; generated output must use the guarded external target"]
fn native_legacy_ui_install_and_hardcoded_dist_plugin_stay_external() {
    let external = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").expect("guarded target"));
    // The guarded target must be an absolute path outside this source tree.
    assert!(external.is_absolute() && !external.starts_with(env!("CARGO_MANIFEST_DIR")));
    let previous = std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT");
    std::env::set_var("THRULINE_VISUAL_CARGO_TARGET_ROOT", &external);
    let mut target = CargoTarget::from_env("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    target.prepare().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let ui = source.join("ui");
    std::fs::create_dir_all(ui.join("dist")).unwrap();
    std::fs::write(ui.join("dist/index.html"), "exact tracked old UI").unwrap();
    std::fs::write(ui.join("package.json"), r#"{"name":"native-routing-fixture","version":"1.0.0","scripts":{"build":"node build.js"}}"#).unwrap();
    std::fs::write(ui.join("package-lock.json"), r#"{"name":"native-routing-fixture","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"native-routing-fixture","version":"1.0.0"}}}"#).unwrap();
    std::fs::write(ui.join("build.js"), r#"const fs=require('fs'),p=require('path');const out=process.argv[process.argv.indexOf('--outDir')+1];fs.writeFileSync(p.join(out,'index.html'),'generated index');fs.mkdirSync('dist/fonts',{recursive:true});fs.writeFileSync('dist/fonts/fixture.txt','generated font');"#).unwrap();
    let cache = temp.path().join("npm-cache");
    std::fs::create_dir_all(cache.join("_cacache")).unwrap();
    let result = build_external_ui(
        &source,
        &ui,
        &target,
        &cache,
        &VisualProfile::for_stage(),
        false,
    );
    if let Some(value) = previous {
        std::env::set_var("THRULINE_VISUAL_CARGO_TARGET_ROOT", value);
    } else {
        std::env::remove_var("THRULINE_VISUAL_CARGO_TARGET_ROOT");
    }
    let workspace = match result {
        Ok(value) => value,
        Err(reason) => panic!("legacy external build failed: {reason}"),
    };
    assert!(workspace.dist.starts_with(target.target.join("ui-build")));
    assert_eq!(ui.join("dist").canonicalize().unwrap(), workspace.dist);
    assert_eq!(
        std::fs::read(workspace.dist.join("index.html")).unwrap(),
        b"generated index"
    );
    assert_eq!(
        std::fs::read(workspace.dist.join("fonts/fixture.txt")).unwrap(),
        b"generated font"
    );
    workspace.verify().unwrap();
    assert!(workspace
        .preserved_files
        .iter()
        .all(VerifiedFile::remains_verified));
    std::fs::write(ui.join("build.js"), "changed exact source").unwrap();
    assert!(workspace.verify().is_err());
}
