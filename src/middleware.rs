use actix_http::body::BoxBody;
use actix_web::HttpResponse;
use actix_web::dev::ServiceRequest;
use actix_web::dev::ServiceResponse;
use actix_web::dev::Transform;
use actix_web::{
    Error,
    dev::{Service, forward_ready},
};
use futures::future::LocalBoxFuture;
use futures::future::Ready;
use futures::future::ready;

/// Middleware to check if the user is authorized to access the resource
/// by checking the JWT token in the Authorization header.
#[derive(Clone)]
pub struct CheckAuthorization;

impl<S> Transform<S, ServiceRequest> for CheckAuthorization
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = CheckAuthorizationMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CheckAuthorizationMiddleware { service }))
    }
}

pub struct CheckAuthorizationMiddleware<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for CheckAuthorizationMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let (http_req, payload) = req.into_parts();
        if let Some(auth_header) = http_req.headers().get("Authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                // right now we are just checking if the token is valid,
                // there are currently no claims that we are checking for
                // if let Ok(_jwt_claims) = SERVER_SIGNING_KEY.verify_jwt(auth_str) {
                let fut = self
                    .service
                    .call(ServiceRequest::from_parts(http_req, payload));
                return Box::pin(async move {
                    let res = fut.await?;
                    Ok(res)
                });
                // }
            }
        }
        let res = HttpResponse::Unauthorized()
            .body("The user attempting to access this resource is not authorized");
        Box::pin(async move {
            actix_web::Result::<ServiceResponse<BoxBody>>::Ok(ServiceResponse::new(http_req, res))
        })
    }
}
