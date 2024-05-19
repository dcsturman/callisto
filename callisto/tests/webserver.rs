use hyper::{client, Uri};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use mockstream::SyncMockStream;
use tokio::process::{ Command, Child };
use tokio::time::{sleep, Duration};

const SERVER_PATH: &str = "./target/debug/callisto";

async fn spawn_test_server() -> Child {
    let daemon = Command::new(SERVER_PATH)
    .kill_on_drop(true)
    .spawn()
    .expect("Daemon failed to start.");

    sleep(Duration::from_millis(1000)).await;

    return daemon;
}

#[tokio::test]
async fn test_simple_get() {
    let mut _daemon = spawn_test_server().await;
    let url = "http://127.0.0.1:3000";

    let body = reqwest::get(url)
    .await.unwrap()
    .text()
    .await.unwrap();

    assert_eq!(body, "[]");
}

#[tokio::test]
async fn test_simple_unknown() {
    let mut _daemon = spawn_test_server();

    // Parse our URL...
    let url = "http://127.0.0.1:3000/unknown";

    let response = reqwest::get(url)
    .await.unwrap();
    assert!(matches!(response, Err(e)));
}
