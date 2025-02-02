#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Lark(#[from] open_lark::core::error::LarkAPIError),
}
