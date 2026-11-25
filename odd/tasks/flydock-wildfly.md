# FlyDock: WildFly prototype

Windows-first desktop prototype using Rust and egui.

## Specs

- S1: "Registrar tu instalación WildFly 32 y tu JDK 21, sin limitar la aplicación a esas versiones."
- S2: "Elegir `standalone.xml` y validar las rutas."
- S3: "Iniciar y detener WildFly desde FlyDock."
- S4: "Ver la salida y el estado del servidor."
- S5: "Conservar el registro al cerrar y volver a abrir la aplicación."
- S6: "sin alterar los archivos del servidor"; "si al iniciarlo detecta que el puerto de administración ya está ocupado, no lanzará una segunda instancia ni finalizará un proceso sin identificarlo."
- S7: "Rust con egui (recomendado)"

## Tasks

- T1 [S1, S2, S5, S7] Build application foundation and persistent server registration. Route: worker. Commit: pending.
- T2 [S3, S4, S6] Implement safe WildFly lifecycle and log display. Route: worker. Commit: pending.
- T3 [S1-S7] Verify integrated prototype and document usage. Route: verify. Commit: pending.

## Log

- L1: "este es un repo vacio, quiero que armemos la idea de un proyecto. necesito desarrollar con rust, una aplicacion de escritorio lo mas liviana posible imitando funciones especificas de jboss tools. necesito levantar servidores wildfly con deployments de maven." The user additionally provided detailed requirements and images in the original conversation.
- L2: "partamos con wildfly, es lo que mas uso"
- L3: "quiero que flydock mande, igual no tendre ambos corriendo"
- L4: The user accepted the first prototype and selected Windows first and Rust with egui.
- L5: Windows rustc and cargo 1.100.0, stable x86_64-pc-windows-msvc, observed in this session.
