use super::{
    ExecPlan, HealthStatus, ListQuery, ListResponse, ProviderError, ProviderManifest,
    SessionDetail,
};

pub trait Provider: Send + Sync {
    fn manifest(&self) -> ProviderManifest;
    fn health(&self) -> HealthStatus;
    fn list_sessions(&self, query: &ListQuery) -> Result<ListResponse, ProviderError>;
    fn session_detail(&self, native_id: &str) -> Result<SessionDetail, ProviderError>;
    fn resume_command(&self, native_id: &str) -> Result<ExecPlan, ProviderError>;
}
