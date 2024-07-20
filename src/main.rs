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
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use futures_util::{SinkExt, TryFutureExt, TryStreamExt};
use askama::Template;
use axum::extract::Query;
use log::{info, log, warn};
use meilisearch_sdk::{MatchRange, SearchResult};
use serde_json::Value;
use tower_http::cors::Vary;
use tower_http::services::ServeFile;
mod templates;

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
struct Product {
    #[serde(rename(deserialize = "_id"))]
    pub id : Option<String>,
    #[serde(rename(deserialize = "product_name_de"))]
    pub name: Option<String>,
    #[serde(rename(deserialize = "product_name_en"))]
    pub name_en: Option<String>,
    #[serde(rename(deserialize = "brands"))]
    pub brands : Option<String>,
    #[serde(rename(deserialize = "ingredients_text_de"))]
    pub ingredients : Option<String>,
    #[serde(rename(deserialize = "ingredients_text_en"))]
    pub ingredients_en: Option<String>
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
        .route("/trigger_delay", get(search))
        .route("/off", get(get_off_items))
        .route("/product", get(product))
        .route_service("/css", ServeFile::new("src/frontend/styles.css"))
}
async fn main_page() -> Html<&'static str> {
    Html(include_str!("frontend/home.html"))
}

async fn search(search_query: Query<SearchQuery>) -> templates::ProductListTemplate {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));
    println!("start search");
    let mut products:Vec<templates::ProductListResult> = vec!();
    match meilisearch_client.index("products")
        .search()
        .with_query(&search_query.search_text)
        .with_show_matches_position(true)
        .execute::<templates::ProductListResult>().await {
        Ok(e) => {
            for mut product in e.hits {
                products.push(templates::ProductListResult {
                    id : product.result.id,
                    name : product.result.name,
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
            if e.total_hits > Some(1) {
                templates::ProductInfoTemplate {
                    id : e.hits.into_iter().nth(0).unwrap().result.id
                }
            }
            else {
                templates::ProductInfoTemplate{ id : 0.to_string() }
            }
        },
        Err(e) => {
            warn!("Unable to locate a razor: {e}, retrying");
            templates::ProductInfoTemplate{ id : 0.to_string() }
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
    let mut products_collection = off_database.collection::<Product>("products");

    let mongo_filter = doc! {"ingredients_analysis_tags" : "en:vegan", "countries_tags.0" : "en:germany"};
    let mut results = match products_collection.find(mongo_filter, FindOptions::builder().limit(1000).build()).await {
        Ok(e) => e,
        Err(_e) => {
            info!("MongoDB Search failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    meilisearch_client.delete_index("products").await.unwrap();

    let meiliesearch_index = meilisearch_client.index("products");

    while let Ok(Some(product)) = results.try_next().await {
        match meiliesearch_index.add_documents(&[product], Option::from("id")).await {
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