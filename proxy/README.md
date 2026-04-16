# nullproxy
Reverse proxy for handling WebTransport client connections to the game server.  
Currently only directly forwards requests between the two, but in the future it should send along the client address details so the server can handle banning/aliasing. The server will need to be modified to handle this data.  

`cargo run --release -- --help`

## Running locally
Generate certificate and private key for localhost:  

`openssl req -new -x509 -nodes -out cert.pem -keyout key.pem -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -subj '/CN=127.0.0.1' -addext "subjectAltName = DNS:localhost" -days 14`

The sha256 fingerprint will be output after running the proxy.  
This array needs to be used in the browser javascript to bypass the certificate on localhost.  
```
const transport = new WebTransport("https://localhost:4433", {
  serverCertificateHashes: [
    {
      algorithm: "sha-256",
      value: new Uint8Array([189, 143, 63, 185, 26, 37, 160, 51, 108, 112, 28, 131, 119, 187, 229, 173, 72, 144, 147, 57, 32, 137, 54, 205, 168, 130, 246, 238, 66, 199, 189, 58]),
    }
  ]
});
```
