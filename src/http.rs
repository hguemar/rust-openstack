// Copyright 2017 Dmitry Tantsur <divius.inside@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implementation details related to HTTP.

use std::marker::PhantomData;

use futures::{Async, future, Future, Poll, Stream};
use futures::stream::Concat2;
use hyper::{Body, Chunk, Client as HyperClient, Request, Response};
use hyper::client::FutureResponse;
use hyper_rustls::HttpsConnector;
use tokio_core::reactor::Handle;

use super::ApiError;


/// Type of HTTP(s) client.
pub struct Client {
    /// Instance of Hyper client used with this client instance.
    pub inner: HyperClient<HttpsConnector>
}

/// API response low-level object.
pub struct ApiResponse(FutureResponse);

/// Result of an API call.
pub struct ApiResult<T> {
    inner: ApiResponse,
    body: Option<Concat2<Body>>,
    _marker: PhantomData<T>
}

/// Trait representing something that can be converted from a Body.
pub trait ParseBody: Sized {
    /// Parse the value from the Body.
    fn parse_body(body: &[u8]) -> Result<Self, ApiError>;
}


const DEFAULT_DNS_THREADS: usize = 4;

impl Client {
    /// Create an HTTP(s) client.
    pub fn new(io_handle: &Handle) -> Client {
        let connector = HttpsConnector::new(DEFAULT_DNS_THREADS, io_handle);
        Client {
            inner: HyperClient::configure()
                .connector(connector)
                .build(io_handle)
        }
    }

    /// Send a request.
    pub fn request(self, request: Request<Body>) -> ApiResponse {
        ApiResponse(self.inner.request(request))
    }
}

impl Future for ApiResponse {
    type Item = Response;
    type Error = ApiError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let resp = try_ready!(self.0.poll());

        let status = resp.status();
        if status.is_success() {
            Ok(Async::Ready(resp))
        } else {
            Err(ApiError::HttpError(status, resp))
        }
    }
}

impl<T> ApiResult<T> {
    /// New result from a response.
    pub fn new(response: ApiResponse) -> ApiResult<T> {
        ApiResult {
            inner: response,
            body: None,
            _marker: PhantomData
        }
    }
}

impl<T> Future for ApiResult<T> where T: ParseBody {
    type Item = T;
    type Error = ApiError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Cache the body to avoid running self.inner.poll() more than once
        if self.body.is_none() {
            let resp = try_ready!(self.inner.poll());
            self.body = Some(resp.body().concat2());
        }

        let maybe_body = self.body.as_mut().unwrap();
        let chunk = try_ready!(maybe_body.poll());
        ParseBody::parse_body(&chunk).map(Async::Ready)
    }
}
