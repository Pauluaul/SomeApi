use askama_axum::Template;
use serde_derive::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "result_product.html")]
pub struct ProductListTemplate {
    pub products : Vec<ProductListResult>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProductListResult {
    pub id: Option<String>, //also the EAN
    pub name: Option<String>,
    #[serde(skip)]
    pub matches_position: Vec<String>
}

#[derive(Template, Deserialize)]
#[template(path = "product.html")]
pub struct ProductInfoTemplate {
    pub id : String
}