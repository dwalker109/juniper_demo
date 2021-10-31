use hyper::{
    server::Server,
    service::{make_service_fn, service_fn},
    Body, Method, Response, StatusCode,
};
use juniper::{
    graphql_object, EmptySubscription, FieldResult, GraphQLInputObject, GraphQLObject, RootNode,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    ops::Deref,
    sync::{Arc, Mutex},
};

#[derive(GraphQLObject, Clone)]
#[graphql(description = "Basics of a service def")]
struct Srv {
    name: String,
    desc: String,
}
#[derive(GraphQLInputObject)]
#[graphql(description = "Basics of a service def")]
struct NewSrv {
    name: String,
    desc: String,
}

struct Database(Mutex<HashMap<i32, Srv>>);

impl Database {
    pub fn new() -> Self {
        let mut inner = HashMap::new();
        inner.insert(
            1,
            Srv {
                name: "Traffic Routing".into(),
                desc: "Mongo... sorry.".into(),
            },
        );
        inner.insert(
            2,
            Srv {
                name: "Main".into(),
                desc: "Here be dragons.".into(),
            },
        );

        Self(Mutex::new(inner))
    }

    pub fn get(&self, id: i32) -> Option<Srv> {
        self.0.lock().unwrap().get(&id).cloned()
    }

    pub fn add(&self, data: &NewSrv) -> Srv {
        let mut k = 0;
        while self.0.lock().unwrap().contains_key(&k) {
            k += 1;
        }

        self.0.lock().unwrap().insert(
            k,
            Srv {
                name: data.name.clone(),
                desc: data.desc.clone(),
            },
        );

        self.0.lock().unwrap().get(&k).unwrap().deref().clone()
    }
}

#[derive(Clone)]
struct Context {
    db: Arc<Database>,
}

impl juniper::Context for Context {}

struct Query {}

#[graphql_object(context = Context)]
impl Query {
    fn apiVersion() -> &'static str {
        "0.1.0"
    }

    fn srv(context: &Context, id: i32) -> FieldResult<Srv> {
        Ok(context.db.get(id).ok_or("not found")?)
    }
}

struct Mutation {}

#[graphql_object(context = Context)]
impl Mutation {
    fn createSrv(context: &Context, data: NewSrv) -> FieldResult<Srv> {
        Ok(context.db.add(&data))
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = ([127, 0, 0, 1], 3000).into();

    let ctx = Arc::new(Context {
        db: Arc::new(Database::new()),
    });

    let root_node = Arc::new(RootNode::new(
        Query {},
        Mutation {},
        EmptySubscription::<Context>::new(),
    ));

    let new_service = make_service_fn(move |_| {
        let root_node = root_node.clone();
        let ctx = ctx.clone();

        async {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let root_node = root_node.clone();
                let ctx = ctx.clone();
                async {
                    Ok::<_, Infallible>(match (req.method(), req.uri().path()) {
                        (&Method::GET, "/") => juniper_hyper::graphiql("/graphql", None).await,
                        (&Method::GET, "/graphql") | (&Method::POST, "/graphql") => {
                            juniper_hyper::graphql(root_node, ctx, req).await
                        }
                        _ => {
                            let mut response = Response::new(Body::empty());
                            *response.status_mut() = StatusCode::NOT_FOUND;
                            response
                        }
                    })
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(new_service);
    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e)
    }
}
