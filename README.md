# nullspace-rs
Subspace client implementation in Rust. Not yet functional. 

# Running
## Web
The wasm target uses WebGPU to render, so it requires a secure context. This can either be localhost or an https environment.  
WebGPU has limited browser support currently. It will run on major browsers, but should have wide support in the future.  

It also requires running the proxy to handle the WebTransport connection. Run this on the same server as the game server.  
This requires public and private keys. If running this locally, specific openssl generation settings are required for a temporary certificate with the correct curve. See the proxy readme for more info. The sha-256 hash needs to be set in the `src/net/webtransport_socket.rs` file for the client build.  

Once rendering is implemented, the web server should have a graphics folder containing the game's base graphics.  
The wasm target will request the graphics from it during client load.  

# Building
## Web
The wasm32 toolchain can be installed with `rustup target add wasm32-unknown-unknown`  
Install the requirements with `cargo install wasm-pack wasm-opt`  
Build with `wasm-pack build --target web --out-dir www/scripts`   
The www folder contains the required files to run on a server.  
A local test server can be ran by installing `cargo install static-web-server` and running `static-web-server -d www/` 

It builds with all of the panic paths still inside. There's probably a way to strip that from the wasm output, but I don't know how.  
