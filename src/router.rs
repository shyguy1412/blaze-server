use url::Url;

use crate::{
    Result,
    api::{Endpoint, GET_HELLO_WORLD},
};

blaze_macros::as_api!(use crate::api);

pub async fn route(_url: Url) -> Result<Endpoint> {
    Ok(GET_HELLO_WORLD)
}
