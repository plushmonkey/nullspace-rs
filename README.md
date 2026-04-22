# nullspace-rs
Subspace client implementation in Rust. Only connects and spectates a random person.  

## Running
Requires the graphics folder from Continuum to be in the same directory. All of the graphics files must be bm2, except ships and tiles must be png.  
If running this on the web, the graphics folder should be in the root directory of the web server. The client will fetch these from `https://server/graphics/`.  

There's no login screen implemented yet, so the login details have to be configured depending on the platform.  

### Executable
Run the exe with `--help` to see the different login details that can be configured.  
Example: `cargo run -- -u test -p pass --ip 127.0.0.1 --port 5000`

### Web
The wasm target uses WebGPU to render, so it requires a secure context. This can either be localhost or an https environment.  
WebGPU has limited browser support currently. It will run on major browsers, but should have wide support in the future.  

It also requires running the proxy to handle the WebTransport connection. Run this on the same server as the game server.  
This requires public and private keys. If running this locally, specific openssl generation settings are required for a temporary certificate with the correct curve. See the proxy readme for more info.  

Modify `www/index.html` to setup the proxy info and login details.  

## Building
### Web
The wasm32 toolchain can be installed with `rustup target add wasm32-unknown-unknown`  
Install the requirements with `cargo install wasm-pack wasm-opt`  
Build the proxy in the `proxy` folder and execute it on the same server as the game server.  
Running this in a non-https environment requires executing the proxy and copying the hash output to `hash` in `www/index.html`.  

Build with `wasm-pack build --target web --out-dir www/scripts/nullspace`   
The www folder contains the required files to run on a server.  
A local test server can be ran by installing `cargo install static-web-server` and running `static-web-server -d www/`  

It builds with all of the panic paths still inside. There's probably a way to strip that from the wasm output, but I don't know how.  
