/// Pagination parameters extracted from query string.
#[derive(Debug)]
pub struct PaginationParams {
    pub page: Option<String>,
    pub limit: usize,
}

impl PaginationParams {
    /// Decode the opaque page token to an offset. Returns 0 for first page.
    pub fn offset(&self) -> usize {
        self.page
            .as_ref()
            .and_then(|p| {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(p)
                    .ok()?;
                let s = String::from_utf8(bytes).ok()?;
                s.parse::<usize>().ok()
            })
            .unwrap_or(0)
    }

    /// Encode an offset as an opaque page token.
    pub fn encode_offset(offset: usize) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(offset.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_round_trip() {
        let encoded = PaginationParams::encode_offset(42);
        let params = PaginationParams {
            page: Some(encoded),
            limit: 10,
        };
        assert_eq!(params.offset(), 42);
    }

    #[test]
    fn offset_none_is_zero() {
        let params = PaginationParams {
            page: None,
            limit: 10,
        };
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn offset_invalid_is_zero() {
        let params = PaginationParams {
            page: Some("garbage".into()),
            limit: 10,
        };
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn offset_round_trip_large_value() {
        let encoded = PaginationParams::encode_offset(999);
        let params = PaginationParams {
            page: Some(encoded),
            limit: 10,
        };
        assert_eq!(params.offset(), 999);
    }
}
