use ristsmart_netem::forwarder::{ForwarderConfig, UdpForwarder};
use ristsmart_netem::ns::NetworkNamespace;
use std::net::SocketAddr;
use std::sync::Arc;

fn privileged() -> bool {
    std::env::var("RISTS_PRIV")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn test_bidirectional_feedback() {
    if !privileged() {
        eprintln!("RISTS_PRIV=1 required to run this test");
        return;
    }

    let mut ns_obj = NetworkNamespace::new("feed-test".to_string(), 51);
    if let Err(e) = ns_obj.create().await {
        eprintln!("Skipping test: failed to create namespace: {}", e);
        return;
    }
    let (connection, handle, _) = match rtnetlink::new_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test: failed to create netlink connection: {}", e);
            return;
        }
    };
    tokio::spawn(connection);
    if let Err(e) = ns_obj.create_veth_pair(&handle).await {
        eprintln!("Skipping test: failed to create veth pair: {}", e);
        return;
    }
    if let Err(e) = ns_obj.configure_addresses(&handle).await {
        eprintln!("Skipping test: failed to configure addresses: {}", e);
        return;
    }
    let ns = Arc::new(ns_obj);

    // Host echo server
    let host_ip = ns.host_ip;
    let echo = tokio::spawn(async move {
        let sock = tokio::net::UdpSocket::bind((host_ip, 7001)).await.unwrap();
        let mut buf = [0u8; 64];
        if let Ok((len, addr)) = sock.recv_from(&mut buf).await {
            let _ = sock.send_to(&buf[..len], addr).await;
        }
    });

    // Forwarder bridging namespace <-> host
    let mut fwd = UdpForwarder::new(ForwarderConfig {
        src_port: 6001,
        dst_host: host_ip.to_string(),
        dst_port: 7001,
    });
    fwd.start(ns.clone()).await.unwrap();

    // Client inside namespace
    let ns_clone = ns.clone();
    let socket = ns
        .with_netns(move || {
            let sock = std::net::UdpSocket::bind((ns_clone.ns_ip, 0)).unwrap();
            sock.set_nonblocking(true).unwrap();
            Ok(tokio::net::UdpSocket::from_std(sock).unwrap())
        })
        .await
        .unwrap();

    let forwarder_addr = SocketAddr::from((ns.ns_ip, 6001));
    socket.send_to(b"hello", forwarder_addr).await.unwrap();
    let mut buf = [0u8; 64];
    let (len, _) = socket.recv_from(&mut buf).await.unwrap();
    assert_eq!(&buf[..len], b"hello");

    fwd.stop().await.unwrap();
    let _ = echo.await;
    ns.cleanup().await.unwrap();
}
