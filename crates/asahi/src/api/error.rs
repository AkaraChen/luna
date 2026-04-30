use rocket::{
    Request, Response,
    http::Status,
    response::{self, Responder},
    serde::json::Json,
};
use serde::Serialize;

use crate::store::StoreError;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::InvalidInput(message) => Self::BadRequest(message),
            StoreError::IssueNotFound(locator) => Self::NotFound(locator),
            StoreError::LockPoisoned => Self::Internal(error.to_string()),
        }
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let (status, error) = match self {
            Self::BadRequest(error) => (Status::BadRequest, error),
            Self::NotFound(error) => (Status::NotFound, error),
            Self::Internal(error) => (Status::InternalServerError, error),
        };

        Response::build_from(Json(ErrorBody { error }).respond_to(request)?)
            .status(status)
            .ok()
    }
}
