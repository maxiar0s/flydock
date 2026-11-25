use flydock::registration::{Registration, RegistrationStore, validate_registration};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "flydock-registration-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("wildfly/standalone/configuration")).unwrap();
        fs::create_dir_all(path.join("wildfly/bin")).unwrap();
        fs::create_dir_all(path.join("jdk/bin")).unwrap();
        fs::write(path.join("wildfly/bin/standalone.bat"), "@echo off").unwrap();
        fs::write(
            path.join("wildfly/standalone/configuration/standalone.xml"),
            "<server/>",
        )
        .unwrap();
        #[cfg(windows)]
        fs::write(path.join("jdk/bin/java.exe"), "fixture").unwrap();
        #[cfg(not(windows))]
        fs::write(path.join("jdk/bin/java"), "fixture").unwrap();
        Self(path)
    }

    fn registration(&self) -> Registration {
        Registration {
            wildfly_home: self.0.join("wildfly").to_string_lossy().into_owned(),
            java_home: self.0.join("jdk").to_string_lossy().into_owned(),
            standalone_config: self
                .0
                .join("wildfly/standalone/configuration/standalone.xml")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_config(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "<server/>").unwrap();
}

#[test]
fn validates_and_persists_registration_without_touching_installation() {
    let fixture = Fixture::new();
    let registration = fixture.registration();
    validate_registration(&registration).unwrap();

    let store = RegistrationStore::new(fixture.0.join("app-data/registrations.json"));
    store.save(&registration).unwrap();
    assert_eq!(store.load().unwrap(), Some(registration));
    assert_eq!(
        fs::read_to_string(
            fixture
                .0
                .join("wildfly/standalone/configuration/standalone.xml")
        )
        .unwrap(),
        "<server/>"
    );
}

#[test]
fn rejects_missing_paths_and_config_outside_wildfly_without_changing_saved_data() {
    let fixture = Fixture::new();
    let store = RegistrationStore::new(fixture.0.join("app-data/registrations.json"));
    let valid = fixture.registration();
    store.save(&valid).unwrap();
    let before = fs::read(store.path()).unwrap();

    let mut missing_java = valid.clone();
    missing_java.java_home = fixture.0.join("missing-jdk").to_string_lossy().into_owned();
    assert!(validate_registration(&missing_java).is_err());
    assert!(store.save(&missing_java).is_err());
    assert_eq!(fs::read(store.path()).unwrap(), before);

    let outside = fixture.0.join("external/standalone.xml");
    write_config(&outside);
    let mut outside_config = valid;
    outside_config.standalone_config = outside.to_string_lossy().into_owned();
    assert!(validate_registration(&outside_config).is_err());
    assert!(store.save(&outside_config).is_err());
    assert_eq!(fs::read(store.path()).unwrap(), before);
}

#[cfg(windows)]
#[test]
fn rejects_a_directory_without_the_windows_wildfly_marker() {
    let fixture = Fixture::new();
    let marker = fixture.0.join("wildfly/bin/standalone.bat");
    fs::write(&marker, "@echo off").unwrap();
    let registration = fixture.registration();
    validate_registration(&registration).unwrap();

    fs::remove_file(marker).unwrap();
    assert!(validate_registration(&registration).is_err());
}

#[test]
fn removes_failed_rename_temporary_file_and_allows_retry() {
    let fixture = Fixture::new();
    let store = RegistrationStore::new(fixture.0.join("app-data/registrations.json"));
    let registration = fixture.registration();
    fs::create_dir_all(store.path()).unwrap();
    fs::write(store.path().join("blocker"), "keep").unwrap();
    let mut temporary = store.path().to_path_buf();
    temporary.set_extension(format!("{}.tmp", std::process::id()));

    assert!(store.save(&registration).is_err());
    assert!(
        !temporary.exists(),
        "failed save must remove its temporary file"
    );

    fs::remove_dir_all(store.path()).unwrap();
    store.save(&registration).unwrap();
    assert_eq!(store.load().unwrap(), Some(registration));
}

#[test]
fn reports_no_registration_when_store_does_not_exist() {
    let fixture = Fixture::new();
    let store = RegistrationStore::new(fixture.0.join("missing/registrations.json"));
    assert_eq!(store.load().unwrap(), None);
}
