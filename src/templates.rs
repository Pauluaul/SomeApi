use askama_axum::Template;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Template)]
#[template(path = "result_product.html")]
pub struct ProductListTemplate {
    pub products : Vec<ProductListResult>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProductListResult {
    pub id: Option<String>, //also the EAN
    pub name: Option<String>,
    pub front_image: Option<String>,
    #[serde(skip)]
    pub matches_position: Vec<String>
}

#[derive(Template, Serialize, Deserialize)]
#[template(path = "product.html")]
pub struct ProductInfoTemplate {
    pub name : Option<String>,
    pub ingredients : Option<String>,
    pub front_image : Option<String>
}