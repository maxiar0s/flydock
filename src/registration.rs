use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    pub wildfly_home: String,
    pub java_home: String,
    pub standalone_config: String,
}

#[derive(Debug)]
pub struct RegistrationError(String);

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RegistrationError {}

fn invalid(message: impl Into<String>) -> RegistrationError {
    RegistrationError(message.into())
}

fn canonical_directory(value: &str, label: &str) -> Result<PathBuf, RegistrationError> {
    let path = Path::new(value);
    let canonical = fs::canonicalize(path).map_err(|_| {
        invalid(format!(
            "{label} no existe o no se puede acceder: {}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(invalid(format!(
            "{label} debe ser una carpeta: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

pub fn validate_registration(registration: &Registration) -> Result<(), RegistrationError> {
    let wildfly_home = canonical_directory(&registration.wildfly_home, "WildFly Home")?;
    #[cfg(windows)]
    if !wildfly_home.join("bin/standalone.bat").is_file() {
        return Err(invalid(format!(
            "No se encontró bin/standalone.bat en la carpeta WildFly: {}",
            wildfly_home.display()
        )));
    }
    let java_home = canonical_directory(&registration.java_home, "JDK Home")?;

    #[cfg(windows)]
    let java_executable = java_home.join("bin/java.exe");
    #[cfg(not(windows))]
    let java_executable = java_home.join("bin/java");
    if !java_executable.is_file() {
        return Err(invalid(format!(
            "No se encontró el ejecutable de Java: {}",
            java_executable.display()
        )));
    }

    let config_path = Path::new(&registration.standalone_config);
    let config = fs::canonicalize(config_path).map_err(|_| {
        invalid(format!(
            "standalone.xml no existe o no se puede acceder: {}",
            config_path.display()
        ))
    })?;
    if !config.is_file() {
        return Err(invalid(format!(
            "standalone.xml debe ser un archivo: {}",
            config.display()
        )));
    }
    if config.file_name().and_then(|name| name.to_str()) != Some("standalone.xml") {
        return Err(invalid(
            "El archivo de configuración debe llamarse standalone.xml",
        ));
    }
    if !config.starts_with(&wildfly_home) {
        return Err(invalid(
            "standalone.xml debe estar dentro de la carpeta de WildFly",
        ));
    }
    Ok(())
}

pub struct RegistrationStore {
    path: PathBuf,
}

impl RegistrationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<Registration>, RegistrationError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(invalid(format!("No se pudo leer el registro: {error}"))),
        };
        serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| invalid(format!("El registro guardado no es válido: {error}")))
    }

    pub fn save(&self, registration: &Registration) -> Result<(), RegistrationError> {
        validate_registration(registration)?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| invalid("Ruta de registro inválida"))?;
        fs::create_dir_all(parent)
            .map_err(|error| invalid(format!("No se pudo crear la carpeta de datos: {error}")))?;
        let mut temporary = self.path.clone();
        let suffix = format!("{}.tmp", std::process::id());
        temporary.set_extension(suffix);
        let bytes = serde_json::to_vec_pretty(registration)
            .map_err(|error| invalid(format!("No se pudo serializar el registro: {error}")))?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| invalid(format!("No se pudo crear el archivo temporal: {error}")))?;
        let _temporary_file = TemporaryFile(temporary.clone());
        write_and_sync_file(
            file,
            &bytes,
            |file, bytes| file.write_all(bytes),
            |file| file.sync_all(),
        )
        .map_err(|error| invalid(format!("No se pudo escribir el registro: {error}")))?;
        fs::rename(&temporary, &self.path)
            .map_err(|error| invalid(format!("No se pudo guardar el registro: {error}")))?;
        Ok(())
    }
}

fn write_and_sync_file(
    mut file: std::fs::File,
    bytes: &[u8],
    write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    sync: impl FnOnce(&std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let result = write(&mut file, bytes).and_then(|()| sync(&file));
    drop(file);
    result
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn default_store_path() -> Result<PathBuf, RegistrationError> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME").or_else(|| {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/share").into_os_string())
    });
    let base = base.ok_or_else(|| invalid("No se encontró una carpeta de datos de usuario"))?;
    Ok(PathBuf::from(base).join("FlyDock/registrations.json"))
}

#[cfg(test)]
mod tests {
    use super::{TemporaryFile, write_and_sync_file};
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write};

    #[test]
    fn sync_failure_closes_and_cleans_the_temporary_file() {
        let directory =
            std::env::temp_dir().join(format!("flydock-sync-failure-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let temporary = directory.join("registrations.tmp");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .unwrap();
        let cleanup = TemporaryFile(temporary.clone());

        let result = write_and_sync_file(
            file,
            b"fixture",
            |file, bytes| file.write_all(bytes),
            |_| {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "simulated sync failure",
                ))
            },
        );
        assert!(result.is_err());
        drop(cleanup);
        assert!(
            !temporary.exists(),
            "failed sync must not leave a temp file"
        );
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn write_failure_closes_and_cleans_the_temporary_file() {
        let directory =
            std::env::temp_dir().join(format!("flydock-write-failure-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let temporary = directory.join("registrations.tmp");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .unwrap();
        let cleanup = TemporaryFile(temporary.clone());

        let result = write_and_sync_file(
            file,
            b"fixture",
            |_, _| {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "simulated write failure",
                ))
            },
            |_| panic!("sync must not run after a write failure"),
        );
        assert!(result.is_err());
        drop(cleanup);
        assert!(
            !temporary.exists(),
            "failed write must not leave a temp file"
        );
        fs::remove_dir(directory).unwrap();
    }
}
