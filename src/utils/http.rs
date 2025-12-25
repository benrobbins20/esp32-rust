use anyhow::{Result, bail};
use embedded_svc::http::client::{Client};
use esp_idf_hal::io::Read;
use esp_idf_svc::http::{Method, client::{EspHttpConnection}};
use esp_idf_svc::http::client::Response;

// http connection structzpub struct HttpClient {
pub struct HttpConn {
    conn: EspHttpConnection,
}

// trait Extract {
//     type Target;

//     fn extract(&mut self) -> &mut Self::Target;
// }

// impl Extract for Client<&mut EspHttpConnection> {
//     type Target = EspHttpConnection;

//     fn extract(&mut self) -> &mut Self::Target {
//         self

//     }
// }

impl HttpConn {
    pub fn new() -> Result<Self> {
        let conn_cfg = esp_idf_svc::http::client::Configuration::default();
        let conn = EspHttpConnection::new(&conn_cfg)?;

        Ok(HttpConn { conn })
    }

    pub fn http_get (&mut self, url: &str) -> Result<()> {
        // create a new client wrapper per GET, borrow connection 

        let mut client = Client::wrap(&mut self.conn);
        let headers = [("accept", "text/plain")];
        
        let req = client.request(Method::Get, url.as_ref(), &headers)?;
        let mut resp = req.submit()?;
        let status_code = resp.status();
        let (resp_headers, reader) = resp.split();
        let ct = resp_headers.header("Content-Type");
        log::info!("Content-Type: {:?}", ct);

        
        match status_code {
            200..=299 => {
                log::info!("HTTP GET successful: {}", status_code);

                Self::read_body(reader)?;
            }
            _ => {bail!("HTTP request failed:")}
        }
    
        Ok(())
    }

    // chunk response body
    fn read_body(reader: &mut impl Read) -> Result<()> {
            
            let mut buf = [0u8; 512]; // buffer for recv chunks
            let mut offset = 0; // offset for handling partial reads
            let mut total = 0; // total bytes read

            // stream the http response body
            loop {
                // if Ok() perform read, silently ignore
                if let Ok(size) = reader.read(&mut buf){
                    // response empty or data exhausted
                    if size == 0 {
                        break;
                    }
                    total += size;
                    let size_plus_offset = size + offset; // offset = 0 99.9999% of the time

                    // handle successful utf8 conversion or parse error
                    match std::str::from_utf8(&buf[..size_plus_offset]) {
                        Ok(text) => {
                            log::info!("Received chunk: {}", text);
                            // always reset offset on successful read
                            offset = 0;
                        }
                        Err(e) => {
                            let valid_to = e.valid_up_to(); // handy utf8 error method to find where failure occurred

                            // unsafe block to print the valid chunk, for some reason
                            unsafe {
                                print!("<<<<<<<<<<<<<");
                                print!("{}", str::from_utf8_unchecked(&buf[..valid_to]));
                                print!(">>>>>>>>>>>>>");
                            }

                            // prepend the invalid data to start of buffer
                            buf.copy_within(..valid_to, 0); 

                            // add the few invalid bytes to offset to print the proper buffer length
                            offset = size_plus_offset - valid_to; 
                                
                        }
                    }
                }

                // 
            }
            log::info!("Total bytes received: {}", total);
        Ok(())
    }
}