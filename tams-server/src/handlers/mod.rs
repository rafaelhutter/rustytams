pub mod delete_requests;
pub mod flow_props;
pub mod flows;
pub mod objects;
pub mod segments;
pub mod service;
pub mod sources;
pub mod storage;
pub mod webhooks;

use salvo::prelude::*;

use crate::error::AppError;
use tams_store::Store;

/// Get the shared Store from the depot.
pub fn get_store(depot: &mut Depot) -> &Store {
    depot.obtain::<Store>().expect("Store not in depot")
}

/// Extract flowId path parameter.
pub fn flow_id(req: &Request) -> String {
    req.param::<String>("flowId")
        .expect("flowId path param missing from route")
}

/// Extract tag name path parameter.
pub fn tag_name(req: &Request) -> String {
    req.param::<String>("name")
        .expect("name path param missing from route")
}

/// Parse JSON from request body, writing a 400 error to the response on failure.
pub async fn parse_json<T: serde::de::DeserializeOwned>(
    req: &mut Request,
    res: &mut Response,
) -> Option<T> {
    match req.parse_json().await {
        Ok(v) => Some(v),
        Err(_) => {
            AppError::bad_request("Invalid JSON body").write_to(res);
            None
        }
    }
}
