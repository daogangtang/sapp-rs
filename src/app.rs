use std::str;
use std::io::Read;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::clone::Clone;

use hyper::status::StatusCode;
use hyper::server::{Handler, Server, Request, Response};
use mime_types::Types as MimeTypes;


pub use typemap::Key;
pub use hyper::header::Headers;
pub use hyper::header;
pub use hyper::mime;
pub use hyper::client::Client;
pub use request::SapperRequest;
pub use response::SapperResponse;
pub use router_m::Router;
pub use router::SapperRouter;
pub use handler::SapperHandler;


/// Path parameter type
#[derive(Clone)]
pub struct PathParams;

/// Re-export Status Codes
pub mod status {
    pub use hyper::status::StatusCode as Status;
    pub use hyper::status::StatusCode::*;
}

/// Sapper error enum
#[derive(Debug, PartialEq, Clone)]
pub enum Error {
    InvalidConfig,
    InvalidRouterConfig,
    FileNotExist,
    NotFound,
    Break,          // 400
    Unauthorized,   // 401
    Forbidden,      // 403
    TemporaryRedirect(String),     // 307
    Custom(String),
    CustomHtml(String),
    CustomJson(String),
    CustomResponse(SapperResponse),
}

/// Sapper result struct
pub type Result<T> = ::std::result::Result<T, Error>; 

/// Sapper module trait
/// 3 methods: before, after, router
pub trait SapperModule: Sync + Send {

    /// module before filter, will be executed before handler
    fn before(&self, req: &mut SapperRequest) -> Result<()> {
        Ok(())
    }
    
    /// module after filter, will be executed after handler
    fn after(&self, req: &SapperRequest, res: &mut SapperResponse) -> Result<()> {
        Ok(())
    }
    
    /// module router method, used to write router collection of this module here
    fn router(&self, &mut SapperRouter) -> Result<()>;
    
}

/// Sapper appshell trait, used to place global before and after middlewares
pub trait SapperAppShell {
    fn before(&self, &mut SapperRequest) -> Result<()>;
    fn after(&self, &SapperRequest, &mut SapperResponse) -> Result<()>;
}

type GlobalInitClosure = Box<Fn(&mut SapperRequest) -> Result<()> + 'static + Send + Sync>;
type SapperAppShellType = Box<SapperAppShell + 'static + Send + Sync>;

/// Sapper app struct
pub struct SapperApp {
    // listen ip address
    pub address:        String,
    // listen port
    pub port:           u32,
    // for app entry, global middeware
    pub shell:          Option<Arc<SapperAppShellType>>,
    // routers actually use to recognize
    pub routers:        Router,
    // do simple static file service
    pub static_service: bool,
    // if need init something, put them here
    pub init_closure:   Option<Arc<GlobalInitClosure>>,
    // 404 not found page
    pub not_found:      Option<String>
}



impl SapperApp {
    pub fn new() -> SapperApp {
        SapperApp {
            address: String::new(),
            port: 0,
            shell: None,
            routers: Router::new(),
            static_service: true,
            init_closure: None,
            not_found: None
        }
    }
    
    // listen ip address
    pub fn address(&mut self, address: &str) -> &mut Self {
        self.address = address.to_owned();
        self
    }
    
    // listen port
    pub fn port(&mut self, port: u32) -> &mut Self {
        self.port = port;
        self
    }
    
    // do simple static file service
    pub fn static_service(&mut self, open: bool) -> &mut Self {
        self.static_service = open;
        self
    }

    // with global middleware shell
    pub fn with_shell(&mut self, w: SapperAppShellType) -> &mut Self {
        self.shell = Some(Arc::new(w));
        self
    }
    
    // init something, usually in global scope
    pub fn init_global(&mut self, clos: GlobalInitClosure) -> &mut Self {
        self.init_closure = Some(Arc::new(clos));
        self
    }

    // define 404 not found page here
    pub fn not_found_page(&mut self, page: String) -> &mut Self {
        self.not_found = Some(page);
        self
    }
    
    // add routers of one module to global routers
    pub fn add_module(&mut self, sm: Box<SapperModule>) -> &mut Self {
        
        let mut router = SapperRouter::new();
        // get the sm router
        sm.router(&mut router).unwrap();
        let sm = Arc::new(sm);
        
        for (method, handler_vec) in router.into_router() {
            // add to wrapped router
            for &(glob, ref handler) in handler_vec.iter() {
                let method = method.clone();
                let glob = glob.clone();
                let handler = handler.clone();
                let sm = sm.clone();
                let shell = self.shell.clone();
                let init_closure = self.init_closure.clone();
                
                self.routers.route(method, glob, Arc::new(Box::new(move |req: &mut SapperRequest| -> Result<SapperResponse> {
                    if let Some(ref c) = init_closure {
                        c(req)?; 
                    }
                    if let Some(ref shell) = shell {
                        shell.before(req)?;
                    }
                    sm.before(req)?;
                    let mut response: SapperResponse = handler.handle(req)?;
                    sm.after(req, &mut response)?;
                    if let Some(ref shell) = shell {
                        shell.after(req, &mut response)?;
                    }
                    Ok(response)
                })));
            }
        }

        self
    }
    
    // run http server
    pub fn run_http(self) {
        
        let addr = self.address.clone() + ":" + &self.port.to_string();
        //let self_box = Arc::new(Box::new(self));

        Server::http(&addr[..]).unwrap()
                .handle(self).unwrap();
       
    }
}


impl Handler for SapperApp {
    /// do actual handling for a request
    fn handle(&self, req: Request, mut res: Response) {
        
        let mut sreq = SapperRequest::new(Box::new(req));
        let (path, query) = sreq.uri();

        // pass req to routers, execute matched biz handler
        let response_w = self.routers.handle_method(&mut sreq, &path);
        match response_w {
            Ok(sres) => {
                *res.status_mut() = sres.status();
                match sres.body() {
                    &Some(ref vec) => {
                        for header in sres.headers().iter() {
                            res.headers_mut()
                                .set_raw(header.name().to_owned(), 
                                         vec![header.value_string().as_bytes().to_vec()]);
                        }
                        return res.send(&vec[..]).unwrap();
                    },
                    &None => {
                        return res.send(&"".as_bytes()).unwrap();
                    }
                }
            },
            Err(Error::NotFound) => {
                if self.static_service {
                    match simple_file_get(&path) {
                        Ok((file_u8vec, file_mime)) => {
                            res.headers_mut().set_raw("Content-Type", vec![file_mime.as_bytes().to_vec()]);
                            return res.send(&file_u8vec[..]).unwrap();
                        },
                        Err(_) => {
                            *res.status_mut() = StatusCode::NotFound;
                            return res.send(self.not_found.to_owned().unwrap_or(String::from("404 Not Found")).as_bytes()).unwrap();
                        }
                    }
                }

                // return 404 NotFound now
                *res.status_mut() = StatusCode::NotFound;
                return res.send(self.not_found.to_owned().unwrap_or(String::from("404 Not Found")).as_bytes()).unwrap();
            },
            Err(Error::Break) => {
                *res.status_mut() = StatusCode::BadRequest;
                return res.send(&"Bad Request".as_bytes()).unwrap();
            },
            Err(Error::Unauthorized) => {
                *res.status_mut() = StatusCode::Unauthorized;
                return res.send(&"Unauthorized".as_bytes()).unwrap();
            },
            Err(Error::Forbidden) => {
                *res.status_mut() = StatusCode::Forbidden;
                return res.send(&"Forbidden".as_bytes()).unwrap();
            },
            Err(Error::TemporaryRedirect(new_uri)) => {
                *res.status_mut() = StatusCode::TemporaryRedirect;
                res.headers_mut().set_raw("Location", vec![new_uri.as_bytes().to_vec()]);
                return res.send(&"Temporary Redirect".as_bytes()).unwrap();
            },
            Err(Error::Custom(ustr)) => {
                *res.status_mut() = StatusCode::Ok;
                return res.send(&ustr.as_bytes()).unwrap();
            },
            Err(Error::CustomHtml(html_str)) => {
                *res.status_mut() = StatusCode::Ok;
                res.headers_mut().set_raw("Content-Type", vec!["text/html".as_bytes().to_vec()]);
                return res.send(&html_str.as_bytes()).unwrap();
            },
            Err(Error::CustomJson(json_str)) => {
                *res.status_mut() = StatusCode::Ok;
                res.headers_mut().set_raw("Content-Type", vec!["application/x-javascript".as_bytes().to_vec()]);
                return res.send(&json_str.as_bytes()).unwrap();
            },
            Err(Error::CustomResponse(response_entity)) => {
                *res.status_mut() = response_entity.status();
                res.headers_mut().clone_from(response_entity.headers());
                return match *response_entity.body() {
                    Some(ref data) => res.send(data).unwrap(),
                    None => {
                        let data = vec![0u8; 0];
                        res.send(data.as_ref()).unwrap()
                    }
                };
            }
            Err(_) => {
                *res.status_mut() = StatusCode::InternalServerError;
                return res.send(&"InternalServerError".as_bytes()).unwrap();
            }

        }
    }
}


// this is very expensive in time
// should make it as global 
lazy_static! {
    static ref MTYPES: MimeTypes = { MimeTypes::new().unwrap() };
}

// a simple static file service
fn simple_file_get(path: &str) -> Result<(Vec<u8>, String)> {
    let new_path;
    if &path[(path.len()-1)..] == "/" {
        new_path = "static/".to_owned() + path + "index.html";
    }
    else {
        new_path = "static/".to_owned() + path;
    }
    //println!("file path: {}", new_path);
    match File::open(&new_path) {
        Ok(ref mut file) => {
            let mut s: Vec<u8> = vec![];
            file.read_to_end(&mut s).unwrap_or(0);
            
            let mt_str = MTYPES.mime_for_path(Path::new(&new_path));
            
            Ok((s, mt_str.to_owned()))
        },
        Err(_) => Err(Error::FileNotExist)
    }
}


