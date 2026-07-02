use okf::voyage::VoyageConfig;

pub(crate) fn api_key_status(config: &VoyageConfig) -> &'static str {
    if config.has_api_key() {
        "configured"
    } else {
        "missing"
    }
}

pub(crate) fn planning_message(config: &VoyageConfig) -> &'static str {
    if config.has_api_key() {
        "Voyage AI is configured. This planning endpoint does not spend tokens."
    } else {
        "OKF_VOYAGE_API_KEY is missing. Planning still works and does not spend tokens."
    }
}
