use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = "8081".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-p" || arg == "--port" {
            if let Some(p) = args.next() {
                port = p;
            }
        }
    }

    let bind_addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Mock target listening on {}", bind_addr);
    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = [0; 1024];
            // Тримаємо з'єднання відкритим у циклі!
            while let Ok(n) = socket.read(&mut buf).await {
                if n == 0 {
                    break;
                } // Троодон закрив з'єднання
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                if socket.write_all(response.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}
