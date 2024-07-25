mod database;

use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::io::read_to_string;
use axum::{Json, Router, routing::get};
use std::net::SocketAddr;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use reqwest;
use meilisearch_sdk::client::Client;
use mongodb::{Client as MongoClient, options::ClientOptions};
use mongodb::bson::{Bson, doc};
use mongodb::options::FindOptions;
use futures_util::{SinkExt, TryFutureExt, TryStreamExt};
use askama::Template;
use axum::extract::{Path, Query};
use log::{info, log, warn};
use meilisearch_sdk;
use serde_json::Value;
use tower_http::cors::Vary;
use tower_http::services::ServeFile;
use std::fs::File;
use std::io::copy;
use std::iter::Map;
use std::ops::Deref;
use tokio::io::AsyncWriteExt;
use rust_i18n::{i18n, t};

mod templates;

i18n!("src/locales");

#[derive(Serialize, Deserialize)]
pub struct SearchQuery {
    search_text: String,
    language : String
}

#[derive(Serialize, Deserialize)]
pub struct InfoQuery {
    id: String
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct OffProduct {
    #[serde(rename(deserialize = "_id"))]
    pub id : Option<String>,
    #[serde(rename(deserialize = "product_name_de"))]
    pub name: Option<String>,
    #[serde(rename(deserialize = "product_name_en"))]
    pub name_en: Option<String>,
    #[serde(rename(deserialize = "brands"))]
    pub brands : Option<String>,
    #[serde(rename(deserialize = "ingredients_text_de"))]
    pub ingredients_de : Option<String>,
    #[serde(rename(deserialize = "ingredients_text_en"))]
    pub ingredients_en: Option<String>,
    pub images: Option<ImageType>
}

impl OffProduct {
    async fn to_vfb(&self) -> VFBProduct {
        VFBProduct {
            id: self.id.clone(),
            name: self.name.clone(),
            name_en: self.name_en.clone(),
            brands: self.brands.clone(),
            ingredients_de: self.ingredients_de.clone(),
            ingredients_en: self.ingredients_en.clone(),
            front_image: download_image(&self.id, &self.images).await
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct VFBProduct {
    pub id : Option<String>,
    pub name: Option<String>,
    pub name_en: Option<String>,
    pub brands : Option<String>,
    pub ingredients_de : Option<String>,
    pub ingredients_en: Option<String>,
    pub front_image: Option<String>
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct ImageType {
    pub front_de: Option<ImageData>,
    pub front_en: Option<ImageData>
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct ImageData {
    pub imgid: Option<Value>
}


#[tokio::main]
async fn main() {
    let app = Router::new()
        .merge(routes_dynamic());
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn routes_dynamic() -> Router {
    Router::new()
        .route("/", get(main_page))
        .route("/:locale", get(main_page))
        .route("/trigger_delay", get(search))
        .route("/off", get(get_off_items))
        .route("/product", get(product))
        .route_service("/css", ServeFile::new("src/frontend/styles.css"))
}
async fn main_page(locale: Option<Path<templates::Locales>>) -> templates::HomeTemplate {
    let locale = locale.unwrap_or(Path(templates::Locales::de)).0;
    templates::HomeTemplate {
        search: t!("search", locale = locale.stringify()).to_string(),
        locale: locale //no shorthand wegen übersicht
    }
}

async fn search(search_query: Query<SearchQuery>) -> templates::ProductListTemplate {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));
    println!("start search");
    let mut products:Vec<templates::ProductListResult> = vec!();
    match meilisearch_client.index("products")
        .search()
        .with_query(&search_query.search_text)
        .with_show_matches_position(true)
        .execute::<VFBProduct>().await {
        Ok(e) => {
            for mut product in e.hits {
                products.push(templates::ProductListResult {
                    id : product.result.id,
                    name : {
                        if product.result.name.clone().is_some_and(|s| s.len() > 0) {
                            product.result.name
                        } else {
                            product.result.name_en
                        }
                    },
                    front_image: product.result.front_image,
                    matches_position : product.matches_position.unwrap().into_iter().map(|(key, value)| key).collect()
                });
                println!("{:?}", products)
            }
        },
        Err(e) => {
            warn!("Unable to locate a razor: {e}, retrying");
            panic!("OH-NO")
        }
    };
    templates::ProductListTemplate { products }
}

async fn product(info_query: Query<InfoQuery>) -> impl IntoResponse {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));
    println!("start search");
    match meilisearch_client.index("products")
        .search()
        .with_query(&info_query.id)
        .with_show_matches_position(true)
        .execute::<templates::ProductInfoTemplate>().await {
        Ok(e) => {
            e.hits.into_iter().nth(0).unwrap().result
        },
        Err(e) => {
            warn!("Unable to locate a razor: {e}, retrying");
            templates::ProductInfoTemplate{
                name: Option::from("error".to_string()),
                ingredients: Option::from("".to_string()),
                front_image: Option::from("".to_string()),
            }
        }
    }
}

async fn get_off_items() -> impl IntoResponse {
    info!("Start indexing");

    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));

    let mongo_client_options = match ClientOptions::parse("mongodb://localhost:27017").await {
        Ok(e) => e,
        Err(_e) => {
            info!("MongoDB client connection");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    let mongo_client = match MongoClient::with_options(mongo_client_options) {
        Ok(database) => database,
        Err(_e)  => {
            info!("MongoDB client option creation");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    let mut off_database = mongo_client.database("off");
    let mut products_collection = off_database.collection::<OffProduct>("products");

    let mongo_filter = doc! {"ingredients_analysis_tags" : "en:vegan", "countries_tags.0" : "en:germany"};
    let mut results = match products_collection.find(mongo_filter).limit(100).await {
        Ok(e) => e,
        Err(_e) => {
            info!("MongoDB Search failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    meilisearch_client.delete_index("products").await.unwrap();

    let meiliesearch_index = meilisearch_client.index("products");

    while let Ok(Some(mut product)) = results.try_next().await {
        let vfb_product = product.to_vfb().await;
        match meiliesearch_index.add_documents(&[vfb_product], Option::from("id")).await {
            Err(_e) => {
                {
                    info!("Meilisearch index failed");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            },
            _ => ()
        };
    };
    info!("Indexing complete");
    StatusCode::OK.into_response()
}

async fn download_image(product: &Option<String>, images: &Option<ImageType>) -> Option<String>
{
    let image_type = match images {
        Some(image_type) => image_type,
        None => &ImageType { front_de: None, front_en: None }
    };

    let mut image_id_option = Value::String("1".to_string());

    if image_type.front_de.is_some() {
        image_id_option = image_type.front_de.clone().unwrap().imgid.unwrap();
    } else if image_type.front_en.is_some() {
        image_id_option = image_type.front_en.clone().unwrap().imgid.unwrap();
    }

    let image_id = match image_id_option {
        Value::Number(number) => number.as_u64().unwrap().to_string(),
        Value::String(string) => string,
        _ => "1".to_string()
    };

    let mut product_id = product.clone().unwrap().to_owned();
    if product_id.len() > 9
    {
        product_id.insert(9, '/');
        product_id.insert(6, '/');
        product_id.insert(3, '/');
    } else if product_id.len() > 8 {
        product_id.insert(6, '/');
        product_id.insert(3, '/');
    }
    return Some(format!("https://openfoodfacts-images.s3.eu-west-3.amazonaws.com/data/{}/{}.400.jpg", product_id, image_id.to_string().replace("\"", "")));
}