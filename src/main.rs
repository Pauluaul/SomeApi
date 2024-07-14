mod database;

use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::io::read_to_string;
use axum::{routing::get, Router, Json};
use std::net::SocketAddr;
use axum::http::{StatusCode};
use axum::response::{Html, IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::{Row};
use reqwest;
use meilisearch_sdk::client::{Client};
use mongodb::{Client as MongoClient, options::ClientOptions};
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use futures_util::{SinkExt, TryFutureExt, TryStreamExt};
use askama::Template;
use axum::extract::Query;
use log::{info, warn};
use meilisearch_sdk::{MatchRange, SearchResult};
use serde_json::Value;
use tower_http::cors::Vary;
use tower_http::services::ServeFile;

#[derive(Serialize, Deserialize)]
pub struct QueryParams {
    search_text: String
}

#[derive(Serialize, Deserialize, Debug)]
struct Product {
    id: Option<String>, //also the EAN
    german_name: Option<String>,
    ingredients : Option<String>,
    quantity : Option<String>,
    #[serde(skip)]
    matches_position: Vec<String>
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct OffProduct {
    #[serde(rename(deserialize = "_id"))]
    pub id : Option<String>,
    #[serde(rename(deserialize = "product_name_de"))]
    pub german_name : Option<String>,
    #[serde(rename(deserialize = "product_name_en"))]
    pub english_name : Option<String>,
    #[serde(rename(deserialize = "brands"))]
    pub brands : Option<String>,
    #[serde(rename(deserialize = "ingredients_text"))]
    pub ingredients : Option<String>,
    #[serde(rename(deserialize = "ingredients_text_de"))]
    pub ingredients_de : Option<String>
}

#[derive(Template)]
#[template(path = "product.html")]
struct ProductTemplate  {
    pub products : Vec<Product>
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .merge(routes_dynamic());
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn routes_dynamic() -> Router {
    Router::new()
        .route("/", get(main_page))
        .route("/trigger_delay", get(search))
        .route_service("/css", ServeFile::new("src/frontend/styles.css"))
}
async fn main_page() -> Html<&'static str> {
    Html(include_str!("frontend/index.html"))
}

async fn search(search_query: Query<QueryParams>) -> ProductTemplate {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));

    let mut products:Vec<Product> = vec!();
    match meilisearch_client.index("products")
        .search()
        .with_query(&search_query.search_text)
        .with_show_matches_position(true)
        .execute::<Product>().await {
        Ok(e) => {
            for mut product in e.hits {
                products.push(Product {
                    id : product.result.id,
                    german_name : product.result.german_name,
                    ingredients: None,
                    quantity: None,
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
    ProductTemplate { products }
}
async fn get_off_items() -> impl IntoResponse {
    println!("start.");

    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));

    let mongo_client_options = match ClientOptions::parse("mongodb://localhost:27017").await {
        Ok(e) => e,
        Err(e) => panic!("client option mongo failed")
    };

    let mongo_client = match MongoClient::with_options(mongo_client_options) {
        Ok(database) => database,
        Err(e)  => panic!("client mongo failed")
    };

    let mut off_database = mongo_client.database("off");
    let mut products_collection = off_database.collection::<OffProduct>("products");

    let mongo_filter = doc! {"ingredients_analysis_tags" : "en:vegan", "countries_tags" : "en:germany"};
    let mut results = match products_collection.find(mongo_filter, FindOptions::builder().build()).await {
        Ok(e) => e,
        Err(e) => panic!("mongo search failed")
    };

    meilisearch_client.delete_index("products").await.unwrap();

    let meiliesearch_index = meilisearch_client.index("products");

    while let Ok(Some(product)) = results.try_next().await {
        match meiliesearch_index.add_documents(&[product], Option::from("id")).await {
            Err(e) => {
                panic!("")
            },
            _ => ()
        };
    };
    (StatusCode::OK, Json("new_products")).into_response()
}