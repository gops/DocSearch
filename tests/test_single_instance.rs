use docsearch::single_instance::{self, InstanceStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_single_instance_activation_and_messaging() {
    let tmp = tempdir().unwrap();
    let index_dir = tmp.path().join("unique_test_index_1");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path_str = index_dir.to_str().unwrap();

    let sock_path = single_instance::get_endpoint_path(index_path_str);

    // 1. Primary instance starts
    let status = single_instance::check_or_bind(index_path_str).expect("Primary should bind");
    let server = match status {
        InstanceStatus::Primary(s) => s,
        InstanceStatus::AlreadyRunning(_) => panic!("Expected primary instance"),
    };

    let show_called = Arc::new(AtomicBool::new(false));
    let show_flag = show_called.clone();

    let rescan_called = Arc::new(AtomicBool::new(false));
    let rescan_flag = rescan_called.clone();

    let handle = server.start_listener(move |action| match action {
        "SHOW" => {
            show_flag.store(true, Ordering::SeqCst);
        }
        "RESCAN" => {
            rescan_flag.store(true, Ordering::SeqCst);
        }
        _ => {}
    });

    // Allow background thread to start listening
    std::thread::sleep(std::time::Duration::from_millis(100));

    // 2. Second instance attempts to start for the same index path
    let status2 = single_instance::check_or_bind(index_path_str).expect("Secondary should detect primary");
    let mut client = match status2 {
        InstanceStatus::AlreadyRunning(c) => c,
        InstanceStatus::Primary(_) => panic!("Expected already running instance"),
    };

    // 3. Send SHOW command from second instance
    let resp = client.send_action("SHOW").expect("Should send action");
    assert_eq!(resp, "OK");
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(show_called.load(Ordering::SeqCst), "SHOW action should be received by primary");

    // 4. Send RESCAN command
    let resp2 = client.send_action("RESCAN").expect("Should send rescan");
    assert_eq!(resp2, "OK");
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(rescan_called.load(Ordering::SeqCst), "RESCAN action should be received by primary");

    // 5. Send PING command
    let resp3 = client.send_action("PING").expect("Should send ping");
    assert_eq!(resp3, "PONG");

    // 6. Test drop cleanup
    drop(handle);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!sock_path.exists(), "Socket file should be removed on handle drop");
}

#[test]
fn test_stale_socket_recovery() {
    let tmp = tempdir().unwrap();
    let index_dir = tmp.path().join("unique_test_index_2");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path_str = index_dir.to_str().unwrap();

    let sock_path = single_instance::get_endpoint_path(index_path_str);

    // Create parent directory if needed
    if let Some(p) = sock_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }

    // Create a dummy / stale file at sock_path
    std::fs::write(&sock_path, b"stale data").unwrap();
    assert!(sock_path.exists());

    // check_or_bind should detect that nobody responds on this socket, clean it up, and bind as primary
    let status = single_instance::check_or_bind(index_path_str).expect("Should recover and bind");
    match status {
        InstanceStatus::Primary(server) => {
            let handle = server.start_listener(|_| {});
            assert!(sock_path.exists(), "New socket should be bound");
            drop(handle);
        }
        InstanceStatus::AlreadyRunning(_) => panic!("Should not be already running with stale socket"),
    }
}
