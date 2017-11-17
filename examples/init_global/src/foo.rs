
use sapper::Result;
use sapper::Request;
use sapper::Response;
use sapper::SapperModule;
use sapper::SapperRouter;

#[derive(Clone)]
pub struct Foo;

impl Foo {
    // those handlers in module Foo
    fn index(req: &mut Request) -> Result<Response> {
        
        let mut response = Response::new();
        response.write_body("hello, boy!".to_string());
        
        Ok(response)
    }
    
    fn test(req: &mut Request) -> Result<Response> {
        
        let mut response = Response::new();
        response.write_body("hello, test!".to_string());
        
        Ok(response)
    }
    
    fn test_post(req: &mut Request) -> Result<Response> {
        
        println!("in test_post, raw_body: {:?}", req.body());
        
        let mut response = Response::new();
        response.write_body("hello, I'am post!".to_string());
        
        Ok(response)
    }
    
}

// set before, after middleware, and add routers
impl SapperModule for Foo {
    
    fn before(&self, req: &mut Request) -> Result<Option<Response>> {
        println!("{}", "in Foo before.");
        Ok(None)
    }
    
    fn after(&self, req: &Request, res: &mut Response) -> Result<()> {
        println!("{}", "in Foo after.");
        
        Ok(())
    }
    
    // here add routers ....
    fn router(&self, router: &mut SapperRouter) -> Result<()> {

        router.get("/foo", Foo::index);
        router.get("/foo/", Foo::index);
        router.get("/foo/test", Foo::test);
        router.post("/foo/test", Foo::test_post);
        
        Ok(())
        
    }
    
    
}

