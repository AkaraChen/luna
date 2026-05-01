use rocket::{
    Request, Response,
    http::Status,
    response::{self, Responder},
    serde::json::Json,
};
use serde::Serialize;

use crate::service::ServiceError;

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

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::InvalidInput(message) => Self::BadRequest(message),
            ServiceError::IssueNotFound(locator) => Self::NotFound(locator),
            ServiceError::ProjectNotFound(locator) => Self::NotFound(locator),
            ServiceError::WikiNodeNotFound(locator) => Self::NotFound(locator),
            ServiceError::WikiVersionNotFound(version) => Self::NotFound(version),
            ServiceError::NotificationNotFound(id) => Self::NotFound(id),
            ServiceError::Database(_) => Self::Internal(error.to_string()),
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
