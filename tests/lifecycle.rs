use flydock::lifecycle::{
    LifecycleState, LogBuffer, management_port_available, management_port_from_config,
};
use std::net::TcpListener;

#[test]
fn log_buffer_keeps_only_the_newest_bounded_output() {
    let mut logs = LogBuffer::new(11);
    logs.push("first\n");
    logs.push("second\n");
    assert_eq!(logs.as_string(), "rst\nsecond\n");
    assert!(logs.byte_len() <= 11);
}

#[test]
fn occupied_management_port_is_reported_unavailable() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(!management_port_available(port));
}

#[test]
fn free_management_port_is_reported_available() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert!(management_port_available(port));
}

#[test]
fn resolves_custom_management_port_and_socket_offset() {
    let xml = r#"<server><socket-binding-group port-offset="3"><socket-binding name="management-http" port="10001"/></socket-binding-group></server>"#;
    assert_eq!(management_port_from_config(xml, &[]).unwrap(), 10004);
}

#[test]
fn literal_management_port_is_not_overridden_by_java_opts() {
    let xml = r#"<server><socket-binding-group port-offset="0"><socket-binding name="management-http" port="10001"/></socket-binding-group></server>"#;
    assert_eq!(
        management_port_from_config(xml, &["-Djboss.management.http.port=10002"]).unwrap(),
        10001
    );
}

#[test]
fn final_matching_java_opt_wins_when_property_is_configured_as_expression() {
    let xml = r#"<server><socket-binding-group port-offset="0"><socket-binding name="management-http" port="${jboss.management.http.port:9990}"/></socket-binding-group></server>"#;
    assert_eq!(
        management_port_from_config(
            xml,
            &["-Djboss.management.http.port=10001 -Djboss.management.http.port=10002"]
        )
        .unwrap(),
        10002
    );
}

#[test]
fn resolves_management_port_property_override() {
    let xml = r#"<server><socket-binding-group port-offset="${jboss.socket.binding.port-offset:0}"><socket-binding name="management-http" port="${jboss.management.http.port:9990}"/></socket-binding-group></server>"#;
    assert_eq!(
        management_port_from_config(
            xml,
            &["-Djboss.management.http.port=10001 -Djboss.socket.binding.port-offset=5"]
        )
        .unwrap(),
        10006
    );
}

#[test]
fn unknown_management_port_expression_fails_closed() {
    let xml = r#"<server><socket-binding-group port-offset="${missing.offset}"><socket-binding name="management-http" port="${jboss.management.http.port:9990}"/></socket-binding-group></server>"#;
    assert!(management_port_from_config(xml, &[]).is_err());
}

#[test]
fn graceful_stop_fails_closed_when_process_ownership_cannot_be_proved() {
    let mut controller = flydock::lifecycle::LifecycleController::default();
    let error = controller.stop().unwrap_err();
    assert_eq!(
        error,
        "Detención graceful no disponible: FlyDock no puede verificar que el endpoint de administración pertenece al proceso iniciado."
    );
}

#[test]
fn graceful_stop_does_not_connect_to_an_unrelated_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut controller = flydock::lifecycle::LifecycleController::default();

    assert!(controller.stop().is_err());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "disabled stop must not connect to a listener that has not been attributed to FlyDock"
    );
}

#[test]
fn lifecycle_state_does_not_claim_running_after_exit() {
    assert_eq!(LifecycleState::Stopped.label(), "Detenido");
    assert_eq!(LifecycleState::Running.label(), "En ejecución");
    assert_eq!(LifecycleState::Unknown.label(), "Desconocido");
}
