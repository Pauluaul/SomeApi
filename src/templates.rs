use std::collections::HashMap;
use askama_axum::Template;
use serde_derive::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "result_product.html")]
pub struct ProductListTemplate {
    pub products: Vec<ProductListResult>,
    pub matches_with_text: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProductListResult {
    pub id: Option<String>, //also the EAN
    pub name: Option<String>,
    pub front_image: Option<String>,
    #[serde(skip)]
    pub matches_position: HashMap<String, String>,
    pub locale: String
}

#[derive(Template, Serialize, Deserialize, Debug, Clone)]
#[template(path = "product.html")]
pub struct ProductInfoTemplate {
    pub name : Option<String>,
    pub ingredients : Option<String>,
    pub front_image : Option<String>
}

#[derive(Template, Serialize, Deserialize)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub search : String,
    pub locale : Locales
}

#[derive(Serialize, Deserialize)]
#[derive(PartialEq)]
pub enum Locales {
    #[serde(rename(deserialize = "en"))]
    EN,
    #[serde(rename(deserialize = "de"))]
    DE
}

impl Locales {
    pub(crate) fn to_string(&self) -> &str {
        match self {
            Locales::EN => "en",
            Locales::DE => "de"
        }
    }
}