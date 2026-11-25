use crate::registration::{Registration, validate_registration};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::collections::VecDeque;
use std::io::{self, Read};
#[cfg(not(windows))]
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub const MAX_LOG_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleState {
    Stopped,
    Running,
    Unknown,
}

impl LifecycleState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stopped => "Detenido",
            Self::Running => "En ejecución",
            Self::Unknown => "Desconocido",
        }
    }
}

#[derive(Debug)]
pub struct LogBuffer {
    max_bytes: usize,
    bytes: VecDeque<u8>,
}

impl LogBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            bytes: VecDeque::new(),
        }
    }

    pub fn push(&mut self, text: &str) {
        if self.max_bytes == 0 {
            return;
        }
        self.bytes.extend(text.as_bytes());
        while self.bytes.len() > self.max_bytes {
            self.bytes.pop_front();
        }
        while !self.bytes.is_empty() && std::str::from_utf8(self.bytes.make_contiguous()).is_err() {
            self.bytes.pop_front();
        }
    }

    pub fn as_string(&self) -> String {
        String::from_utf8_lossy(&self.bytes.iter().copied().collect::<Vec<_>>()).into_owned()
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

/// Resolves the management listener from the selected XML and supported JAVA_OPTS properties.
/// Unknown expressions are rejected rather than assuming WildFly's default port.
pub fn management_port_from_config(xml: &str, java_opts: &[&str]) -> Result<u16, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut offset = None;
    let mut management_port = None;
    let mut socket_group_count = 0;
    let mut management_binding_count = 0;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let tag = element.name();
                if tag.as_ref() == b"socket-binding-group" {
                    socket_group_count += 1;
                    if socket_group_count > 1 {
                        return Err(
                            "No se puede establecer un único socket-binding-group activo.".into(),
                        );
                    }
                    offset = attribute(&element, b"port-offset", reader.decoder())?;
                } else if tag.as_ref() == b"socket-binding" {
                    let name = attribute(&element, b"name", reader.decoder())?;
                    if name.as_deref() == Some("management-http") {
                        management_binding_count += 1;
                        if management_binding_count > 1 {
                            return Err(
                                "No se puede establecer un único socket-binding management-http."
                                    .into(),
                            );
                        }
                        management_port = attribute(&element, b"port", reader.decoder())?;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("No se pudo interpretar standalone.xml: {error}")),
        }
    }

    let offset = offset
        .ok_or_else(|| "No se pudo establecer el port-offset de administración.".to_owned())?;
    let management_port = management_port
        .ok_or_else(|| "No se encontró el socket-binding management-http.".to_owned())?;
    let offset = resolve_config_number(&offset, "jboss.socket.binding.port-offset", java_opts)?;
    let port = resolve_config_number(&management_port, "jboss.management.http.port", java_opts)?;
    let effective = port
        .checked_add(offset)
        .filter(|port| *port > 0)
        .ok_or_else(|| "El puerto efectivo de administración está fuera de rango.".to_owned())?;
    Ok(effective)
}

fn attribute(
    element: &BytesStart<'_>,
    key: &[u8],
    decoder: quick_xml::encoding::Decoder,
) -> Result<Option<String>, String> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| format!("Atributo XML inválido: {error}"))?;
        if attribute.key.as_ref() == key {
            return attribute
                .decode_and_unescape_value(decoder)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("Valor XML inválido: {error}"));
        }
    }
    Ok(None)
}

fn resolve_config_number(value: &str, property: &str, java_opts: &[&str]) -> Result<u16, String> {
    if !(value.starts_with("${") && value.ends_with('}')) {
        return value
            .parse::<u16>()
            .map_err(|_| format!("El valor configurado para {property} no es numérico."));
    }

    let expression = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| format!("La expresión configurada para {property} no es válida."))?;
    let (name, default) = expression.split_once(':').unwrap_or((expression, ""));
    if name != property {
        return Err(format!(
            "No se puede establecer el valor de {property}: expresión {value:?} no reconocida."
        ));
    }
    let mut override_value = None;
    for opts in java_opts {
        for token in opts.split_whitespace() {
            if let Some(value) = token.strip_prefix(&format!("-D{property}=")) {
                override_value = Some(value);
            }
        }
    }
    match override_value {
        Some(value) => value
            .parse::<u16>()
            .map_err(|_| format!("El valor {property} de JAVA_OPTS no es numérico.")),
        None => default
            .parse::<u16>()
            .map_err(|_| format!("No se pudo establecer un valor numérico para {property}.")),
    }
}

pub fn management_port_available(port: u16) -> bool {
    #[cfg(windows)]
    {
        // netstat covers IPv4/IPv6 and loopback-only listeners; failed or malformed queries fail closed.
        let Ok(output) = Command::new("netstat.exe")
            .args(["-ano", "-p", "tcp"])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let listing = String::from_utf8_lossy(&output.stdout);
        for line in listing.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields
                .first()
                .is_some_and(|protocol| protocol.eq_ignore_ascii_case("TCP"))
            {
                let Some(local_address) = fields.get(1) else {
                    return false;
                };
                if local_address
                    .rsplit_once(':')
                    .and_then(|(_, value)| value.parse::<u16>().ok())
                    == Some(port)
                {
                    return false;
                }
            }
        }
        return true;
    }
    #[cfg(not(windows))]
    {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
            && TcpListener::bind(("0.0.0.0", port)).is_ok()
    }
}

pub struct LifecycleController {
    child: Option<Child>,
    logs: Arc<Mutex<LogBuffer>>,
    last_error: Option<String>,
}

impl Default for LifecycleController {
    fn default() -> Self {
        Self {
            child: None,
            logs: Arc::new(Mutex::new(LogBuffer::new(MAX_LOG_BYTES))),
            last_error: None,
        }
    }
}

impl LifecycleController {
    pub fn state(&mut self) -> LifecycleState {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.child = None;
                    LifecycleState::Stopped
                }
                Ok(None) => LifecycleState::Running,
                Err(error) => {
                    self.last_error = Some(format!(
                        "No se pudo consultar el estado del proceso WildFly: {error}"
                    ));
                    LifecycleState::Unknown
                }
            }
        } else {
            LifecycleState::Stopped
        }
    }

    pub fn logs(&self) -> String {
        self.logs
            .lock()
            .map(|logs| logs.as_string())
            .unwrap_or_default()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn start(&mut self, registration: &Registration) -> Result<(), String> {
        self.last_error = None;
        if self.state() != LifecycleState::Stopped {
            return Err("FlyDock no iniciará otro proceso mientras no pueda confirmar que el anterior terminó.".into());
        }
        validate_registration(registration).map_err(|error| error.to_string())?;
        let config = std::fs::read_to_string(&registration.standalone_config)
            .map_err(|error| format!("No se pudo leer standalone.xml: {error}"))?;
        let java_opts = [
            std::env::var("JAVA_OPTS").unwrap_or_default(),
            std::env::var("JAVA_OPTS_APPEND").unwrap_or_default(),
        ];
        let port = management_port_from_config(&config, &[&java_opts[0], &java_opts[1]])?;
        if !management_port_available(port) {
            let error = format!(
                "El puerto de administración efectivo {port} ya está ocupado o no se pudo comprobar; no se inició WildFly."
            );
            self.last_error = Some(error.clone());
            return Err(error);
        }
        let home = Path::new(&registration.wildfly_home);
        let executable = home.join("bin").join("standalone.bat");
        self.spawn_server(&executable, home, registration)
            .map_err(|error| {
                let message = format!("No se pudo iniciar WildFly: {error}");
                self.last_error = Some(message.clone());
                message
            })
    }

    #[cfg(windows)]
    fn spawn_server(
        &mut self,
        executable: &Path,
        home: &Path,
        registration: &Registration,
    ) -> io::Result<()> {
        let mut child = Command::new("cmd.exe")
            .arg("/D")
            .arg("/C")
            .arg(executable)
            .arg("-c")
            .arg(&registration.standalone_config)
            .current_dir(home)
            .env("JAVA_HOME", &registration.java_home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(stdout) = child.stdout.take() {
            stream_logs(stdout, Arc::clone(&self.logs), "");
        }
        if let Some(stderr) = child.stderr.take() {
            stream_logs(stderr, Arc::clone(&self.logs), "[stderr] ");
        }
        self.child = Some(child);
        Ok(())
    }

    #[cfg(not(windows))]
    fn spawn_server(
        &mut self,
        _executable: &Path,
        _home: &Path,
        _registration: &Registration,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "El ciclo de vida de WildFly está disponible solo en Windows.",
        ))
    }

    pub fn stop(&mut self) -> Result<(), String> {
        let message = "Detención graceful no disponible: FlyDock no puede verificar que el endpoint de administración pertenece al proceso iniciado.".to_owned();
        self.last_error = Some(message.clone());
        Err(message)
    }
}

fn stream_logs(
    mut reader: impl Read + Send + 'static,
    logs: Arc<Mutex<LogBuffer>>,
    prefix: &'static str,
) {
    thread::spawn(move || {
        let mut chunk = [0_u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let text = String::from_utf8_lossy(&chunk[..count]);
                    if let Ok(mut logs) = logs.lock() {
                        logs.push(&format!("{prefix}{text}"));
                    }
                }
            }
        }
    });
}
