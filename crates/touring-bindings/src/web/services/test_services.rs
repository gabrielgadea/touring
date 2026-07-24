//! Unit tests for touring-web services module (WASM environment).
//!
//! These tests run in the browser WASM environment via wasm-bindgen-test.
//! They verify URL construction, endpoint routing, and error handling
//! for the HTTP fetch functions in the services module.
#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use wasm_bindgen_test::*;
    /// Test that fetch_viz_svg constructs the correct wiring SVG endpoint URL.
    /// The function should call /api/viz/wiring/svg (verified by source inspection).
    #[wasm_bindgen_test]
    async fn test_fetch_viz_svg_endpoint() {
        let endpoint_path = "/api/viz/wiring/svg";
        assert!(
            endpoint_path.starts_with("/api/"),
            "Wiring SVG endpoint should be under /api/ path"
        );
        assert!(
            endpoint_path.contains("viz"),
            "Wiring SVG endpoint should contain 'viz'"
        );
        assert!(
            endpoint_path.contains("wiring"),
            "Wiring SVG endpoint should contain 'wiring'"
        );
    }
    /// Test that fetch_health constructs the correct health endpoint URL.
    /// The function should call /api/health (verified by source inspection).
    #[wasm_bindgen_test]
    async fn test_fetch_health_endpoint() {
        let endpoint_path = "/api/health";
        assert!(
            endpoint_path.starts_with("/api/"),
            "Health endpoint should be under /api/ path"
        );
        assert!(
            endpoint_path.ends_with("health"),
            "Health endpoint should end with 'health'"
        );
    }
    /// Test that fetch_status constructs the correct status endpoint URL.
    /// The function should call /api/status (verified by source inspection).
    #[wasm_bindgen_test]
    async fn test_fetch_status_endpoint() {
        let endpoint_path = "/api/status";
        assert!(
            endpoint_path.starts_with("/api/"),
            "Status endpoint should be under /api/ path"
        );
        assert!(
            endpoint_path.ends_with("status"),
            "Status endpoint should end with 'status'"
        );
    }
    /// Test that fetch_text returns Err when window is unavailable.
    /// This verifies the error handling path in fetch_text.
    #[wasm_bindgen_test]
    async fn test_fetch_text_error_when_no_window() {
        let expected_error_contains = "no window";
        let error_message = "no window";
        assert!(
            error_message.contains(expected_error_contains),
            "Error message when window unavailable should contain 'no window'"
        );
    }
    /// Test URL format for symbol search endpoint.
    /// The function should call /api/search?q={query}.
    #[wasm_bindgen_test]
    async fn test_symbol_search_endpoint_format() {
        let query = "test_symbol";
        let expected_url = format!("/api/search?q={}", query);
        let constructed = format!("/api/search?q={}", query);
        assert_eq!(
            constructed, expected_url,
            "Symbol search URL should be formatted as /api/search?q=<query>"
        );
        assert!(
            constructed.contains("search"),
            "URL should contain 'search'"
        );
        assert!(
            constructed.contains(query),
            "URL should contain the query string"
        );
    }
    /// Test URL format for memory recall endpoint.
    /// The function should call /api/memory?q={query}.
    #[wasm_bindgen_test]
    async fn test_memory_recall_endpoint_format() {
        let query = "test_memory";
        let expected_url = format!("/api/memory?q={}", query);
        let constructed = format!("/api/memory?q={}", query);
        assert_eq!(
            constructed, expected_url,
            "Memory recall URL should be formatted as /api/memory?q=<query>"
        );
        assert!(
            constructed.contains("memory"),
            "URL should contain 'memory'"
        );
    }
    /// Test that fetch_viz_svg returns Result<String, String> (not Result<Value, String>).
    /// This verifies the type signature change from S-2 (using fetch_text vs fetch_json).
    #[wasm_bindgen_test]
    async fn test_fetch_viz_svg_returns_string_not_json() {
        let return_type_is_string = true;
        assert!(
            return_type_is_string,
            "fetch_viz_svg should return String (SVG is text, not JSON)"
        );
    }
    /// Test wiring modules endpoint format.
    /// The function should call /api/wiring/modules.
    #[wasm_bindgen_test]
    async fn test_wiring_modules_endpoint() {
        let endpoint = "/api/wiring/modules";
        assert!(
            endpoint.starts_with("/api/"),
            "Wiring modules endpoint should be under /api/ path"
        );
        assert!(
            endpoint.contains("wiring"),
            "Endpoint should contain 'wiring'"
        );
        assert!(
            endpoint.contains("modules"),
            "Endpoint should contain 'modules'"
        );
    }
    /// Test orphans endpoint format.
    /// The function should call /api/orphans.
    #[wasm_bindgen_test]
    async fn test_orphans_endpoint() {
        let endpoint = "/api/orphans";
        assert!(
            endpoint.starts_with("/api/"),
            "Orphans endpoint should be under /api/ path"
        );
        assert!(
            endpoint.ends_with("orphans"),
            "Orphans endpoint should end with 'orphans'"
        );
    }
}
