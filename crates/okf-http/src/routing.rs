use std::collections::BTreeSet;

use axum::{
    body::Body,
    http::Request,
    middleware::{self, Next},
    routing::{delete, get, post, put, MethodRouter},
    Extension, Router,
};

use crate::{
    api_config_root_deprecated, api_config_roots, api_document, api_documents, api_edges_apply,
    api_graph, api_health, api_login, api_pair_session, api_password_change,
    api_review_set_accept_all, api_review_set_deny_all, api_root_configuration,
    api_root_initialization_plan, api_root_initialize, api_root_monitoring_accept,
    api_root_monitoring_check, api_root_monitoring_dismiss, api_root_monitoring_pending,
    api_root_monitoring_status, api_root_proposal_create, api_root_proposal_details,
    api_root_register, api_root_remove, api_root_update, api_session_logout, api_session_refresh,
    api_session_status, api_sessions_revoke, api_sessions_revoke_user, api_suggestion_accept,
    api_suggestion_deny, api_suggestions_generate, api_suggestions_import, api_suggestions_list,
    api_users_list, api_voyage_check, api_voyage_index, api_voyage_plan, api_voyage_rebuild,
    api_voyage_search, api_voyage_status, health, redirect_to_browser, serve_browser_asset,
    serve_legacy_okf_document, serve_legacy_scanlab_document, serve_legacy_scql_document,
    serve_okf_document, serve_repo_file, serve_unmounted_okf_document, AppState, ServerMode,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum AccessClass {
    AnonymousRead,
    AuthenticationBootstrap,
    AuthenticatedSensitiveRead,
    LocalEditorMutation,
    CanonicalWrite,
    DerivedStateMutation,
    TokenSpending,
    SecurityAdministration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RouteContract {
    pub(crate) method: &'static str,
    pub(crate) path: &'static str,
    pub(crate) access: AccessClass,
    pub(crate) modes: &'static [ServerMode],
    pub(crate) capabilities: &'static [&'static str],
    pub(crate) legacy: bool,
}

const ALL_MODES: &[ServerMode] = &[
    ServerMode::ReadOnly,
    ServerMode::LocalEditor,
    ServerMode::AuthenticatedTls,
];
const EDIT_MODES: &[ServerMode] = &[ServerMode::LocalEditor, ServerMode::AuthenticatedTls];
const LOCAL_EDITOR_MODE: &[ServerMode] = &[ServerMode::LocalEditor];
const TLS_MODE: &[ServerMode] = &[ServerMode::AuthenticatedTls];

const CONTENT_READ: &[&str] = &["content.read"];
const ROOTS_PROPOSE: &[&str] = &["roots.propose"];
const ROOTS_CONFIGURE: &[&str] = &["roots.configure"];
const CONTENT_INITIALIZE: &[&str] = &["content.initialize"];
const REVIEW_DECIDE: &[&str] = &["review.decide"];
const DERIVED_REBUILD: &[&str] = &["derived.rebuild"];
const VOYAGE_SPEND: &[&str] = &["voyage.spend"];
const VOYAGE_INDEX: &[&str] = &["voyage.spend", "derived.rebuild"];
const CONTENT_WRITE: &[&str] = &["content.write"];
const SESSION_PAIR: &[&str] = &["session.pair"];
const SESSION_REFRESH: &[&str] = &["session.recover"];
const SESSION_LOGOUT: &[&str] = &["session.logout"];
const SESSION_LOGIN: &[&str] = &["session.login"];
const PASSWORD_CHANGE: &[&str] = &["password.change"];
const USERS_MANAGE: &[&str] = &["users.manage"];
const SECURITY_MANAGE: &[&str] = &["security.manage"];
const KNOWN_CAPABILITIES: &[&str] = &[
    "content.read",
    "roots.propose",
    "roots.configure",
    "content.initialize",
    "content.write",
    "review.decide",
    "derived.rebuild",
    "voyage.spend",
    "users.manage",
    "security.manage",
    "session.logout",
    "session.pair",
    "session.recover",
    "session.login",
    "password.change",
];

struct ClassifiedRouter {
    router: Router<AppState>,
    contracts: Vec<RouteContract>,
    selected_mode: Option<ServerMode>,
}

impl ClassifiedRouter {
    fn new(selected_mode: Option<ServerMode>) -> Self {
        Self {
            router: Router::new(),
            contracts: Vec::new(),
            selected_mode,
        }
    }

    fn add(mut self, contract: RouteContract, route: MethodRouter<AppState>) -> Self {
        self.contracts.push(contract);
        if match self.selected_mode {
            Some(mode) => contract.modes.contains(&mode),
            None => true,
        } {
            let route =
                route.layer(middleware::from_fn(
                    move |Extension(state): Extension<AppState>,
                          request: Request<Body>,
                          next: Next| async move {
                        crate::security::enforce_route_authorization(contract, state, request, next)
                            .await
                    },
                ));
            self.router = self.router.route(contract.path, route);
        }
        self
    }

    fn get(
        self,
        path: &'static str,
        access: AccessClass,
        modes: &'static [ServerMode],
        capabilities: &'static [&'static str],
        legacy: bool,
        route: MethodRouter<AppState>,
    ) -> Self {
        self.add(
            RouteContract {
                method: "GET",
                path,
                access,
                modes,
                capabilities,
                legacy,
            },
            route,
        )
    }

    fn post(
        self,
        path: &'static str,
        access: AccessClass,
        modes: &'static [ServerMode],
        capabilities: &'static [&'static str],
        legacy: bool,
        route: MethodRouter<AppState>,
    ) -> Self {
        self.add(
            RouteContract {
                method: "POST",
                path,
                access,
                modes,
                capabilities,
                legacy,
            },
            route,
        )
    }

    fn put(
        self,
        path: &'static str,
        access: AccessClass,
        modes: &'static [ServerMode],
        capabilities: &'static [&'static str],
        legacy: bool,
        route: MethodRouter<AppState>,
    ) -> Self {
        self.add(
            RouteContract {
                method: "PUT",
                path,
                access,
                modes,
                capabilities,
                legacy,
            },
            route,
        )
    }

    fn delete(
        self,
        path: &'static str,
        access: AccessClass,
        modes: &'static [ServerMode],
        capabilities: &'static [&'static str],
        legacy: bool,
        route: MethodRouter<AppState>,
    ) -> Self {
        self.add(
            RouteContract {
                method: "DELETE",
                path,
                access,
                modes,
                capabilities,
                legacy,
            },
            route,
        )
    }
}

macro_rules! api_get {
    ($routes:expr, $suffix:literal, $access:expr, $modes:expr, $capabilities:expr, $handler:expr) => {{
        $routes
            .get(
                concat!("/api/v1", $suffix),
                $access,
                $modes,
                $capabilities,
                false,
                get($handler),
            )
            .get(
                concat!("/api/okf", $suffix),
                $access,
                $modes,
                $capabilities,
                true,
                get($handler),
            )
    }};
}

macro_rules! api_post {
    ($routes:expr, $suffix:literal, $access:expr, $modes:expr, $capabilities:expr, $handler:expr) => {{
        $routes
            .post(
                concat!("/api/v1", $suffix),
                $access,
                $modes,
                $capabilities,
                false,
                post($handler),
            )
            .post(
                concat!("/api/okf", $suffix),
                $access,
                $modes,
                $capabilities,
                true,
                post($handler),
            )
    }};
}

macro_rules! api_put {
    ($routes:expr, $suffix:literal, $access:expr, $modes:expr, $capabilities:expr, $handler:expr) => {{
        $routes
            .put(
                concat!("/api/v1", $suffix),
                $access,
                $modes,
                $capabilities,
                false,
                put($handler),
            )
            .put(
                concat!("/api/okf", $suffix),
                $access,
                $modes,
                $capabilities,
                true,
                put($handler),
            )
    }};
}

macro_rules! api_delete {
    ($routes:expr, $suffix:literal, $access:expr, $modes:expr, $capabilities:expr, $handler:expr) => {{
        $routes
            .delete(
                concat!("/api/v1", $suffix),
                $access,
                $modes,
                $capabilities,
                false,
                delete($handler),
            )
            .delete(
                concat!("/api/okf", $suffix),
                $access,
                $modes,
                $capabilities,
                true,
                delete($handler),
            )
    }};
}

pub(crate) fn router(scanlab_compat: bool, mode: ServerMode) -> Router<AppState> {
    let classified = classified_router(scanlab_compat, Some(mode));
    validate_contracts(&classified.contracts).expect("valid OKF HTTP route capability matrix");
    classified.router
}

fn classified_router(scanlab_compat: bool, selected_mode: Option<ServerMode>) -> ClassifiedRouter {
    let mut routes = ClassifiedRouter::new(selected_mode)
        .get(
            "/",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(redirect_to_browser),
        )
        .get(
            "/docs-browser",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(redirect_to_browser),
        )
        .get(
            "/docs-browser/",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(redirect_to_browser),
        )
        .get(
            "/docs-browser/{*path}",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(serve_browser_asset),
        )
        .get(
            "/okf-docs/{mount}/{*path}",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(serve_okf_document),
        )
        .get(
            "/okf-root/{index}/{*path}",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(serve_unmounted_okf_document),
        );

    routes = api_get!(
        routes,
        "/documents",
        AccessClass::AnonymousRead,
        ALL_MODES,
        CONTENT_READ,
        api_documents
    );
    routes = api_get!(
        routes,
        "/document",
        AccessClass::AnonymousRead,
        ALL_MODES,
        CONTENT_READ,
        api_document
    );
    routes = api_get!(
        routes,
        "/graph",
        AccessClass::AnonymousRead,
        ALL_MODES,
        CONTENT_READ,
        api_graph
    );
    routes = routes
        .get(
            "/api/v1/access/session",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(api_session_status),
        )
        .post(
            "/api/v1/access/login",
            AccessClass::AuthenticationBootstrap,
            TLS_MODE,
            SESSION_LOGIN,
            false,
            post(api_login),
        )
        .post(
            "/api/v1/access/pair",
            AccessClass::AuthenticationBootstrap,
            LOCAL_EDITOR_MODE,
            SESSION_PAIR,
            false,
            post(api_pair_session),
        )
        .post(
            "/api/v1/access/session/refresh",
            AccessClass::AuthenticationBootstrap,
            EDIT_MODES,
            SESSION_REFRESH,
            false,
            post(api_session_refresh),
        )
        .post(
            "/api/v1/access/logout",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            SESSION_LOGOUT,
            false,
            post(api_session_logout),
        )
        .post(
            "/api/v1/access/password",
            AccessClass::SecurityAdministration,
            TLS_MODE,
            PASSWORD_CHANGE,
            false,
            post(api_password_change),
        )
        .post(
            "/api/v1/access/sessions/revoke",
            AccessClass::SecurityAdministration,
            TLS_MODE,
            SESSION_LOGOUT,
            false,
            post(api_sessions_revoke),
        )
        .get(
            "/api/v1/access/users",
            AccessClass::SecurityAdministration,
            TLS_MODE,
            USERS_MANAGE,
            false,
            get(api_users_list),
        )
        .post(
            "/api/v1/access/sessions/revoke-user",
            AccessClass::SecurityAdministration,
            TLS_MODE,
            SECURITY_MANAGE,
            false,
            post(api_sessions_revoke_user),
        );
    routes = api_get!(
        routes,
        "/config/roots",
        AccessClass::AuthenticatedSensitiveRead,
        EDIT_MODES,
        ROOTS_PROPOSE,
        api_config_roots
    );
    routes = api_post!(
        routes,
        "/config/roots",
        AccessClass::LocalEditorMutation,
        EDIT_MODES,
        ROOTS_CONFIGURE,
        api_config_root_deprecated
    );
    routes = api_put!(
        routes,
        "/config/roots/{index}",
        AccessClass::LocalEditorMutation,
        EDIT_MODES,
        ROOTS_CONFIGURE,
        api_config_root_deprecated
    );
    routes = api_delete!(
        routes,
        "/config/roots/{index}",
        AccessClass::LocalEditorMutation,
        EDIT_MODES,
        ROOTS_CONFIGURE,
        api_config_root_deprecated
    );
    routes = routes
        .get(
            "/api/v1/roots/configuration",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            get(api_root_configuration),
        )
        .get(
            "/api/v1/roots/monitoring",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            get(api_root_monitoring_status),
        )
        .post(
            "/api/v1/roots/{id}/monitoring/check",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            post(api_root_monitoring_check),
        )
        .get(
            "/api/v1/roots/{id}/monitoring/pending",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            get(api_root_monitoring_pending),
        )
        .post(
            "/api/v1/roots/{id}/monitoring/accept",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            ROOTS_CONFIGURE,
            false,
            post(api_root_monitoring_accept),
        )
        .post(
            "/api/v1/roots/{id}/monitoring/dismiss",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            ROOTS_CONFIGURE,
            false,
            post(api_root_monitoring_dismiss),
        )
        .post(
            "/api/v1/roots/proposals",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            post(api_root_proposal_create),
        )
        .get(
            "/api/v1/roots/proposals/{id}",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            ROOTS_PROPOSE,
            false,
            get(api_root_proposal_details),
        )
        .post(
            "/api/v1/roots/proposals/{id}/registration",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            ROOTS_CONFIGURE,
            false,
            post(api_root_register),
        )
        .post(
            "/api/v1/roots/proposals/{id}/initialization/plan",
            AccessClass::AuthenticatedSensitiveRead,
            EDIT_MODES,
            CONTENT_INITIALIZE,
            false,
            post(api_root_initialization_plan),
        )
        .post(
            "/api/v1/roots/proposals/{id}/initialization",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            CONTENT_INITIALIZE,
            false,
            post(api_root_initialize),
        )
        .put(
            "/api/v1/roots/{id}",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            ROOTS_CONFIGURE,
            false,
            put(api_root_update),
        )
        .delete(
            "/api/v1/roots/{id}",
            AccessClass::LocalEditorMutation,
            EDIT_MODES,
            ROOTS_CONFIGURE,
            false,
            delete(api_root_remove),
        );
    routes = api_get!(
        routes,
        "/voyage/status",
        AccessClass::AnonymousRead,
        ALL_MODES,
        CONTENT_READ,
        api_voyage_status
    );
    routes = api_post!(
        routes,
        "/voyage/plan",
        AccessClass::AnonymousRead,
        ALL_MODES,
        CONTENT_READ,
        api_voyage_plan
    );
    routes = api_post!(
        routes,
        "/voyage/check",
        AccessClass::TokenSpending,
        EDIT_MODES,
        VOYAGE_SPEND,
        api_voyage_check
    );
    routes = api_post!(
        routes,
        "/voyage/index",
        AccessClass::TokenSpending,
        EDIT_MODES,
        VOYAGE_INDEX,
        api_voyage_index
    );
    routes = api_post!(
        routes,
        "/voyage/rebuild",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        DERIVED_REBUILD,
        api_voyage_rebuild
    );
    routes = api_post!(
        routes,
        "/voyage/search",
        AccessClass::TokenSpending,
        EDIT_MODES,
        VOYAGE_SPEND,
        api_voyage_search
    );
    routes = api_get!(
        routes,
        "/suggestions",
        AccessClass::AuthenticatedSensitiveRead,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_suggestions_list
    );
    routes = api_post!(
        routes,
        "/suggestions/generate",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_suggestions_generate
    );
    routes = api_post!(
        routes,
        "/suggestions/import",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_suggestions_import
    );
    routes = api_post!(
        routes,
        "/suggestions/{id}/accept",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_suggestion_accept
    );
    routes = api_post!(
        routes,
        "/suggestions/{id}/deny",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_suggestion_deny
    );
    routes = api_post!(
        routes,
        "/review-sets/{id}/accept-all",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_review_set_accept_all
    );
    routes = api_post!(
        routes,
        "/review-sets/{id}/deny-all",
        AccessClass::DerivedStateMutation,
        EDIT_MODES,
        REVIEW_DECIDE,
        api_review_set_deny_all
    );
    routes = api_post!(
        routes,
        "/edges/apply",
        AccessClass::CanonicalWrite,
        EDIT_MODES,
        CONTENT_WRITE,
        api_edges_apply
    );
    routes = routes
        .get(
            "/api/v1/health",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(api_health),
        )
        .get(
            "/health",
            AccessClass::AnonymousRead,
            ALL_MODES,
            CONTENT_READ,
            false,
            get(health),
        );

    if scanlab_compat {
        routes = routes
            .get(
                "/docs/{*path}",
                AccessClass::AnonymousRead,
                ALL_MODES,
                CONTENT_READ,
                true,
                get(serve_legacy_scanlab_document),
            )
            .get(
                "/crates/scql/docs/knowledge/{*path}",
                AccessClass::AnonymousRead,
                ALL_MODES,
                CONTENT_READ,
                true,
                get(serve_legacy_scql_document),
            )
            .get(
                "/crates/okf/docs/knowledge/{*path}",
                AccessClass::AnonymousRead,
                ALL_MODES,
                CONTENT_READ,
                true,
                get(serve_legacy_okf_document),
            )
            .get(
                "/repo-files/{*path}",
                AccessClass::AnonymousRead,
                ALL_MODES,
                CONTENT_READ,
                true,
                get(serve_repo_file),
            );
    }

    routes
}

fn validate_contracts(contracts: &[RouteContract]) -> Result<(), String> {
    let mut routes = BTreeSet::new();
    for contract in contracts {
        if !contract.path.starts_with('/') {
            return Err(format!("route path must be absolute: {}", contract.path));
        }
        if !matches!(contract.method, "GET" | "POST" | "PUT" | "DELETE") {
            return Err(format!(
                "unsupported method {} for {}",
                contract.method, contract.path
            ));
        }
        if !routes.insert((contract.method, contract.path)) {
            return Err(format!(
                "duplicate route contract: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.modes.is_empty() {
            return Err(format!(
                "route has no access mode: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.capabilities.is_empty() {
            return Err(format!(
                "route has no declared capability: {} {}",
                contract.method, contract.path
            ));
        }
        if let Some(capability) = contract
            .capabilities
            .iter()
            .find(|capability| !KNOWN_CAPABILITIES.contains(capability))
        {
            return Err(format!(
                "route has unknown capability {capability}: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access == AccessClass::AnonymousRead && contract.modes != ALL_MODES {
            return Err(format!(
                "anonymous read route is not available in every mode: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access != AccessClass::AnonymousRead
            && contract.modes.contains(&ServerMode::ReadOnly)
        {
            return Err(format!(
                "sensitive route is available in read-only mode: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access == AccessClass::AuthenticationBootstrap {
            let valid_pairing = contract.modes == LOCAL_EDITOR_MODE
                && contract.capabilities.contains(&"session.pair");
            let valid_login =
                contract.modes == TLS_MODE && contract.capabilities.contains(&"session.login");
            let valid_recovery =
                contract.modes == EDIT_MODES && contract.capabilities.contains(&"session.recover");
            if !valid_pairing && !valid_login && !valid_recovery {
                return Err(format!(
                    "authentication bootstrap route has an invalid mode or capability: {} {}",
                    contract.method, contract.path
                ));
            }
        }
        if contract.access == AccessClass::SecurityAdministration && contract.modes != TLS_MODE {
            return Err(format!(
                "security administration route is not TLS-only: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access == AccessClass::SecurityAdministration
            && !contract.capabilities.iter().any(|capability| {
                matches!(
                    *capability,
                    "security.manage" | "users.manage" | "password.change" | "session.logout"
                )
            })
        {
            return Err(format!(
                "security administration route lacks security.manage: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access == AccessClass::TokenSpending
            && !contract.capabilities.contains(&"voyage.spend")
        {
            return Err(format!(
                "token-spending route lacks voyage.spend: {} {}",
                contract.method, contract.path
            ));
        }
        if contract.access == AccessClass::CanonicalWrite
            && !contract.capabilities.contains(&"content.write")
        {
            return Err(format!(
                "canonical write route lacks content.write: {} {}",
                contract.method, contract.path
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_route_has_one_valid_access_contract() {
        let classified = classified_router(true, None);
        validate_contracts(&classified.contracts).expect("valid route contracts");

        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::AnonymousRead));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::AuthenticationBootstrap));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| { contract.access == AccessClass::AuthenticatedSensitiveRead }));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::LocalEditorMutation));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::CanonicalWrite));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::DerivedStateMutation));
        assert!(classified
            .contracts
            .iter()
            .any(|contract| contract.access == AccessClass::TokenSpending));
        assert!(classified.contracts.iter().any(|contract| {
            contract.access == AccessClass::SecurityAdministration && contract.modes == TLS_MODE
        }));
    }

    #[test]
    fn anonymous_reads_and_sensitive_routes_have_disjoint_mode_contracts() {
        let classified = classified_router(true, None);
        for contract in classified.contracts {
            if contract.access == AccessClass::AnonymousRead {
                assert_eq!(
                    contract.modes, ALL_MODES,
                    "{} {}",
                    contract.method, contract.path
                );
            } else {
                assert!(
                    !contract.modes.contains(&ServerMode::ReadOnly),
                    "{} {}",
                    contract.method,
                    contract.path
                );
            }
        }
    }

    #[test]
    fn read_only_and_local_editor_route_sets_follow_the_matrix() {
        let classified = classified_router(true, None);
        let read_only = classified
            .contracts
            .iter()
            .filter(|contract| contract.modes.contains(&ServerMode::ReadOnly))
            .collect::<Vec<_>>();
        assert!(!read_only.is_empty());
        assert!(read_only
            .iter()
            .all(|contract| contract.access == AccessClass::AnonymousRead));

        let local_editor = classified
            .contracts
            .iter()
            .filter(|contract| contract.modes.contains(&ServerMode::LocalEditor))
            .collect::<Vec<_>>();
        assert!(local_editor.len() > read_only.len());
        assert!(local_editor
            .iter()
            .any(|contract| contract.access == AccessClass::CanonicalWrite));
        assert!(local_editor
            .iter()
            .any(|contract| contract.access == AccessClass::TokenSpending));
        assert!(!local_editor
            .iter()
            .any(|contract| contract.access == AccessClass::SecurityAdministration));
        let login = classified
            .contracts
            .iter()
            .find(|contract| contract.path == "/api/v1/access/login")
            .expect("persistent login route");
        assert_eq!(login.modes, TLS_MODE);
        assert_eq!(login.capabilities, SESSION_LOGIN);
    }

    #[test]
    fn compatibility_routes_are_explicit_and_opt_in() {
        let generic = classified_router(false, None);
        assert!(generic
            .contracts
            .iter()
            .filter(|contract| contract.legacy)
            .all(|contract| contract.path.starts_with("/api/okf/")));

        let compatible = classified_router(true, None);
        let compatibility_paths = compatible
            .contracts
            .iter()
            .filter(|contract| {
                contract.legacy
                    && !contract.path.starts_with("/api/okf/")
                    && !matches!(contract.path, "/" | "/health")
            })
            .map(|contract| contract.path)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            compatibility_paths,
            BTreeSet::from([
                "/crates/okf/docs/knowledge/{*path}",
                "/crates/scql/docs/knowledge/{*path}",
                "/docs/{*path}",
                "/repo-files/{*path}",
            ])
        );
    }

    #[test]
    fn route_contract_validation_rejects_unsafe_matrices() {
        let anonymous_mutation = RouteContract {
            method: "POST",
            path: "/unsafe",
            access: AccessClass::LocalEditorMutation,
            modes: ALL_MODES,
            capabilities: ROOTS_CONFIGURE,
            legacy: false,
        };
        assert!(validate_contracts(&[anonymous_mutation]).is_err());

        let token_without_capability = RouteContract {
            method: "POST",
            path: "/spend",
            access: AccessClass::TokenSpending,
            modes: EDIT_MODES,
            capabilities: &[],
            legacy: false,
        };
        assert!(validate_contracts(&[token_without_capability]).is_err());

        let duplicate = RouteContract {
            method: "GET",
            path: "/duplicate",
            access: AccessClass::AnonymousRead,
            modes: ALL_MODES,
            capabilities: CONTENT_READ,
            legacy: false,
        };
        assert!(validate_contracts(&[duplicate, duplicate]).is_err());

        let unclassified_capability = RouteContract {
            method: "POST",
            path: "/unclassified",
            access: AccessClass::LocalEditorMutation,
            modes: EDIT_MODES,
            capabilities: &[],
            legacy: false,
        };
        assert!(validate_contracts(&[unclassified_capability]).is_err());

        let unknown_capability = RouteContract {
            method: "POST",
            path: "/unknown-capability",
            access: AccessClass::DerivedStateMutation,
            modes: EDIT_MODES,
            capabilities: &["unknown.capability"],
            legacy: false,
        };
        assert!(validate_contracts(&[unknown_capability]).is_err());

        let remotely_available_pairing = RouteContract {
            method: "POST",
            path: "/pair-remotely",
            access: AccessClass::AuthenticationBootstrap,
            modes: EDIT_MODES,
            capabilities: SESSION_PAIR,
            legacy: false,
        };
        assert!(validate_contracts(&[remotely_available_pairing]).is_err());
    }
}
