# nullspace-rs
Subspace client implementation in Rust. Not yet functional. 

# Building
## Web
The wasm32 toolchain can be installed with `rustup target add wasm32-unknown-unknown`  
Install the requirements with `cargo install wasm-pack wasm-opt`  
Build with `wasm-pack build --target web --out-dir www/scripts`   
The www folder contains the required files to run on a server.  
A local test server can be ran by installing `cargo install static-web-server` and running `static-web-server -d www/` 

It builds with all of the panic paths still inside. There's probably a way to strip that from the wasm output, but I don't know how.  
