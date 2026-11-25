# FlyDock

FlyDock is a Windows-first Rust/egui desktop foundation for registering a WildFly installation and JDK. The initial prototype validates and persists a local registration; it does not start or stop server processes and does not edit WildFly files.

## Requirements

- Rust toolchain (stable)
- Windows is the primary target; other platforms can be used for development
- A WildFly installation and a JDK of your choice (the app does not restrict their versions)

## Run

```powershell
cargo run
```

Use **Examinar…** to select the WildFly home, JDK home, and `standalone.xml`. **Validar y guardar registro** checks that both home paths are directories, that the JDK contains its platform Java executable, and that the selected `standalone.xml` is a file inside the selected WildFly home. The saved registration is stored as JSON under the current user's application data directory (`%APPDATA%\FlyDock\registrations.json` on Windows) and is loaded when FlyDock opens again.

## WildFly lifecycle

On Windows, use **Iniciar WildFly** to launch the registered `bin/standalone.bat` with the selected `standalone.xml` and JDK. FlyDock resolves the effective management HTTP port from the selected configuration, including numeric socket-binding offsets and supported `JAVA_OPTS` overrides. If it cannot resolve the port, or Windows cannot confirm that the port is free, startup is refused. The Windows listener check queries all local addresses, including loopback-only listeners. FlyDock never terminates the process occupying that port. The displayed state tracks only the `cmd.exe` process FlyDock launched, and an indeterminate process state prevents another launch. Graceful stop is currently unavailable: FlyDock does not yet track the complete descendant process tree or bind protocol verification and shutdown to one authenticated management connection. A port-to-PID observation alone would not prove that the listener is WildFly's management protocol, and a separate CLI connection could race with a port rebind. A new Windows console/process group could scope `CTRL_BREAK_EVENT` away from the user's console, but group delivery alone does not establish that the JVM will perform a graceful WildFly shutdown; no isolated JVM fixture currently proves that behavior. FlyDock therefore will not signal the process group or send `:shutdown` to an endpoint. The disabled control explains this limitation. FlyDock never force-kills a process or sends a shutdown request to an unverified server.

The application displays a rolling, bounded tail of up to 64 KiB from the launched process's stdout/stderr. It is an in-memory view and is not retained when the application closes. WildFly's own log files are not edited. The lifecycle controls are Windows-only; elsewhere they report that limitation. No Maven or Docker commands are used.

FlyDock reads the selected paths and writes its registration in its own user data directory. It does not modify server configuration files. Port expressions that cannot be resolved safely cause startup to fail closed; only numeric values and supported numeric defaults/overrides are accepted.
