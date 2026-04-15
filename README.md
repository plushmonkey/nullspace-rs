# nullspace-rs
Subspace client implementation in Rust. Only connects and spectates a random person.  

## Running
Requires the graphics folder from Continuum to be in the same directory. All of the graphics files must be bm2, except ships and tiles must be png.  
If running this on the web, the graphics folder should be in the root directory of the web server. The client will fetch these from `https://server/graphics/`.  

### Web
The wasm target uses WebGPU to render, so it requires a secure context. This can either be localhost or an https environment.  
WebGPU has limited browser support currently. It will run on major browsers, but should have wide support in the future.  

It also requires running the proxy to handle the WebTransport connection. Run this on the same server as the game server.  
This requires public and private keys. If running this locally, specific openssl generation settings are required for a temporary certificate with the correct curve. See the proxy readme for more info.  

## Building
The game address and proxy address/hash are currently compiled directly into the executable.  
They can be modified in `game_connection`, `proxy_url`, and `proxy_hash`. Those files are directly included during the compilation step.  

#### Executable config
`game_connection`: Must be ipv4 address and port separated by `:`, such as `127.0.0.1:5000`.  

#### Web config
`proxy_url`: Must be a url starting with `https://` and ending with a port, such as `https://localhost:4433`.  
`proxy_hash`: Must be the list of numbers output from the proxy after launching it. This is only required if running in non-secure environment.  

### Web
The wasm32 toolchain can be installed with `rustup target add wasm32-unknown-unknown`  
Install the requirements with `cargo install wasm-pack wasm-opt`  
Build the proxy in the `proxy` folder and execute it on the same server as the game server.  
Modify the `proxy_hash` file to points to a valid https url, such as `https://localhost:4433`  
Running this in a non-https environment requires executing the proxy and copying the hash output into `proxy_hash` file in the root of this project. This will automatically be included in the `src/net/webtransport_socket.rs` file to bypass the cert validation.  

Build with `wasm-pack build --target web --out-dir www/scripts`   
The www folder contains the required files to run on a server.  
A local test server can be ran by installing `cargo install static-web-server` and running `static-web-server -d www/`  

It builds with all of the panic paths still inside. There's probably a way to strip that from the wasm output, but I don't know how.  
