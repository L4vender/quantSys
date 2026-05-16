use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RawArchiveHealth {
    pub status: &'static str,
    pub service: &'static str,
}

impl RawArchiveHealth {
    pub fn ok() -> Self {
        Self {
            status: "ok",
            service: "raw-archive",
        }
    }
}
