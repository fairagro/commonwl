use lsp::backend::Backend;
use tokio::net::TcpListener;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    let address = "localhost:9292";
    let listener = TcpListener::bind(address).await.unwrap();
    println!("Listening on {address}");

    let (stream, _) = listener.accept().await.unwrap();
    let (read, write) = tokio::io::split(stream);
    let (service, socket) = LspService::new(Backend::new);
    Server::new(read, write, socket).serve(service).await;
}
