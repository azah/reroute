use hyper::Method;
use hyper::{Body, Request, Response, StatusCode};
use regex::{Regex, RegexSet};
use smallvec::SmallVec;

pub use error::Error;

mod error;

pub type Captures<'r> = Option<SmallVec<[&'r str; 4]>>;
type RouteHandler = Box<dyn Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync>;

/// The Router struct contains the information for your app to route requests
/// properly based on their HTTP method and matching route. It allows the use
/// of a custom 404 handler if desired but provides a default as well.
///
/// Under the hood a Router uses a `RegexSet` to match URI's that come in to the
/// instance of the hyper server. Because of this, it has the potential to match
/// multiple patterns that you provide. It will call the first handler that it
/// matches against so the order in which you add routes matters.
pub struct Router {
    routes: RegexSet,
    patterns: Vec<Regex>,
    handlers: Vec<(Method, RouteHandler)>,
    not_found: RouteHandler,
}

impl Router {
    /// This function should be called inside of a hyper service. It will find the correct handler
    /// for the given route and handle errors appropriately.
    pub fn handle(&self, req: Request<Body>) -> Response<Body> {
        // It should be cheaper to clone this than making an owned string of the path.
        let uri = req.uri().clone();
        let uri = uri.path();
        let matches = self.routes.matches(uri);
        if !matches.matched_any() {
            return (self.not_found)(req, None);
        }

        for index in matches {
            let (ref method, ref handler) = self.handlers[index];
            if method != req.method() {
                continue;
            }

            let regex = &self.patterns[index];
            let captures = get_captures(regex, uri);
            return handler(req, captures);
        }

        not_allowed()
    }
}

/// A `RouterBuilder` enables you to build up a set of routes and their handlers
/// to be handled by a `Router`.
#[derive(Default)]
pub struct RouterBuilder {
    routes: Vec<String>,
    handlers: Vec<(Method, RouteHandler)>,
    not_found: Option<RouteHandler>,
}

impl RouterBuilder {
    /// Create a new `RouterBuilder` with no route handlers.
    pub fn new() -> RouterBuilder {
        RouterBuilder::default()
    }

    /// Install a handler for requests of method `verb` and which have paths
    /// matching `route`. There are also convenience methods named after the
    /// appropriate verb.
    pub fn route<H>(&mut self, verb: Method, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        // Anchor the pattern at the start and end so routes only match exactly.
        let pattern = [r"\A", route, r"\z"].join("");

        self.routes.push(pattern);
        self.handlers.push((verb, Box::new(handler)));

        self
    }

    /// Compile the routes in a `RouterBuilder` to produce a `Router` capable
    /// of handling Hyper requests.
    pub fn finalize(self) -> Result<Router, Error> {
        Ok(Router {
            routes: RegexSet::new(self.routes.iter())?,
            patterns: self
                .routes
                .iter()
                .map(|route| Regex::new(route))
                .collect::<Result<_, _>>()?,
            handlers: self.handlers,
            not_found: self
                .not_found
                .unwrap_or_else(|| Box::new(default_not_found)),
        })
    }

    /// Convenience method to install a GET handler.
    pub fn get<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::GET, route, handler)
    }

    /// Convenience method to install a POST handler.
    pub fn post<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::POST, route, handler)
    }

    /// Convenience method to install a PUT handler.
    pub fn put<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::PUT, route, handler)
    }

    /// Convenience method to install a PATCH handler.
    pub fn patch<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::PATCH, route, handler)
    }

    /// Convenience method to install a DELETE handler.
    pub fn delete<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::DELETE, route, handler)
    }

    /// Convenience method to install an OPTIONS handler.
    pub fn options<H>(&mut self, route: &str, handler: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.route(Method::OPTIONS, route, handler)
    }

    /// Install a fallback handler for when there is no matching route for a
    /// request. If none is installed, the resulting `Router` will use a
    /// default handler.
    pub fn not_found<H>(&mut self, not_found: H) -> &mut RouterBuilder
    where
        H: Fn(Request<Body>, Captures) -> Response<Body> + Send + Sync + 'static,
    {
        self.not_found = Some(Box::new(not_found));
        self
    }
}

// The default 404 handler.
fn default_not_found(_: Request<Body>, _: Captures) -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("Not Found".into())
        .unwrap()
}

// This handler will get fired when a URI matches a route but contains the wrong method.
fn not_allowed() -> Response<Body> {
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body("Method Not Allowed".into())
        .unwrap()
}

// Return that captures from a pattern that was matched.
fn get_captures<'r>(pattern: &'r Regex, uri: &'r str) -> Captures<'r> {
    // We know this compiles because it was part of the set.
    let caps = pattern.captures(uri);
    match caps {
        Some(caps) => {
            let mut v = SmallVec::<[&str; 4]>::new();
            caps.iter()
                .filter(|c| c.is_some())
                .for_each(|c| v.push(c.unwrap().as_str()));
            Some(v)
        }
        None => None,
    }
}

#[test]
fn bad_regular_expression() {
    fn test_handler(_: Request<Body>, _: Captures) -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .body("Ok".into())
            .unwrap()
    }
    let mut router = RouterBuilder::new();
    router.route(Method::GET, r"/[", test_handler);
    let e = router.finalize();
    assert!(e.is_err());
}
