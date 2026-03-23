use salvo::prelude::*;

use tams_types::pagination::PaginationParams;
use tams_types::tags::TagFilters;

/// Extract tag filters from a Salvo request's query string.
pub fn tag_filters_from_request(req: &Request) -> TagFilters {
    tag_filters_from_request_with_prefix(req, "tag")
}

/// Extract tag filters with a custom prefix (e.g. "flow_tag" for flow_tag.{name}).
pub fn tag_filters_from_request_with_prefix(req: &Request, prefix: &str) -> TagFilters {
    req.uri()
        .query()
        .map(|q| TagFilters::from_query_with_prefix(q, prefix))
        .unwrap_or_default()
}

/// Validate that all query parameters are in the allowed set.
/// Tag params (tag.*, tag_exists.*, flow_tag.*, flow_tag_exists.*) and
/// access_token (auth) are always allowed.
pub fn validate_query_params(req: &Request, allowed: &[&str], res: &mut Response) -> bool {
    let Some(query) = req.uri().query() else {
        return true;
    };
    for pair in query.split('&') {
        let key = pair.split('=').next().unwrap_or("");
        if key.is_empty() {
            continue;
        }
        // Always allow auth and tag params
        if key == "access_token"
            || key.starts_with("tag.")
            || key.starts_with("tag_exists.")
            || key.starts_with("flow_tag.")
            || key.starts_with("flow_tag_exists.")
        {
            continue;
        }
        if !allowed.contains(&key) {
            crate::error::AppError::bad_request(format!("Unknown query parameter: {key}"))
                .write_to(res);
            return false;
        }
    }
    true
}

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;

/// Extract pagination params from request query string.
pub fn pagination_from_request(req: &Request) -> PaginationParams {
    let page = req.query::<String>("page");
    let limit = req
        .query::<usize>("limit")
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);
    PaginationParams { page, limit }
}

/// Result of paginating a slice: the current page and optional next-page info.
pub struct PaginationResult<'a, T> {
    pub items: &'a [T],
    pub limit: usize,
    pub next_key: Option<String>,
}

/// Paginate a slice using the given parameters. Pure logic, no framework deps.
pub fn paginate<'a, T>(items: &'a [T], params: &PaginationParams) -> PaginationResult<'a, T> {
    let offset = params.offset();
    let limit = params.limit;

    let page_items = if offset < items.len() {
        let end = (offset + limit).min(items.len());
        &items[offset..end]
    } else {
        &[]
    };

    let next_offset = offset + limit;
    let next_key = if next_offset < items.len() {
        Some(PaginationParams::encode_offset(next_offset))
    } else {
        None
    };

    PaginationResult {
        items: page_items,
        limit,
        next_key,
    }
}

/// Paginate items and set pagination response headers. Returns the current page slice.
pub fn paginate_and_set_headers<'a, T>(
    items: &'a [T],
    params: &PaginationParams,
    req: &Request,
    res: &mut Response,
) -> &'a [T] {
    let result = paginate(items, params);

    res.add_header("x-paging-limit", result.limit.to_string(), true)
        .ok();

    if let Some(ref next_key) = result.next_key {
        res.add_header("x-paging-nextkey", next_key, true).ok();

        // Build Link header, preserving existing query params
        let scheme = req.scheme();
        let authority = req
            .uri()
            .authority()
            .map(|a| a.to_string())
            .or_else(|| req.header::<String>("host"))
            .unwrap_or_else(|| "localhost".to_string());
        let path = req.uri().path();
        let mut query_parts: Vec<String> = req
            .uri()
            .query()
            .unwrap_or("")
            .split('&')
            .filter(|p| !p.is_empty())
            .filter(|p| !p.starts_with("page=") && !p.starts_with("limit="))
            .map(String::from)
            .collect();
        query_parts.push(format!("page={next_key}"));
        query_parts.push(format!("limit={}", result.limit));
        let query_string = query_parts.join("&");
        let link = format!("<{scheme}://{authority}{path}?{query_string}>; rel=\"next\"");
        res.add_header("link", link, true).ok();
    }

    result.items
}
