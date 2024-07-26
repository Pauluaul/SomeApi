use askama_axum::IntoResponse;
use log::{error, info, warn};
use meilisearch_sdk::{Client, MatchRange};
use mongodb::options::ClientOptions;
use axum::http::StatusCode;
use mongodb::Client as MongoClient;
use mongodb::bson::{Bson, doc};
use futures_util::TryStreamExt;
use serde_derive::{Deserialize, Serialize};
use axum::extract::Query;
use std::collections::HashMap;
use rust_i18n::t;
use regex::Regex;
use serde_json::Value;
use crate::{ImageType, InfoQuery, SearchQuery, templates};


#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
struct OffProduct {
    #[serde(rename(deserialize = "_id"))]
    pub id : Option<String>,
    pub product_name_de: Option<String>,
    pub product_name_en: Option<String>,
    pub brands : Option<String>,
    pub ingredients_text_de: Option<String>,
    pub ingredients_text_en: Option<String>,
    pub images: Option<ImageType>,
    pub nutriments: Option<Bson>,
    pub stores_tags: Option<Vec<String>>
}

impl OffProduct {
    async fn to_vfb(&self) -> VFBProduct {
        VFBProduct {
            id: self.id.clone(),
            name_de: self.product_name_de.clone(),
            name_en: self.product_name_en.clone(),
            brands: self.brands.clone(),
            ingredients_de: self.ingredients_text_de.clone(),
            ingredients_en: self.ingredients_text_en.clone(),
            front_image: download_image(&self.id, &self.images).await,
            nutriments: self.nutriments.clone(),
            stores_tags: self.stores_tags.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Clone)]
pub struct VFBProduct {
    pub id : Option<String>,
    pub name_de: Option<String>,
    pub name_en: Option<String>,
    pub brands : Option<String>,
    pub ingredients_de : Option<String>,
    pub ingredients_en: Option<String>,
    pub front_image: Option<String>,
    pub nutriments: Option<Bson>,
    pub stores_tags: Option<Vec<String>>
}

static SEARCHABLE_ATTRIBUTES: [&str; 6] = [
    "id",
    "name_de",
    "name_en",
    "brands",
    "ingredients_de",
    "ingredients_en"
];

pub async fn get_off_items() -> impl IntoResponse {
    info!("Start indexing");

    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));

    let mongo_client_options = match ClientOptions::parse("mongodb://localhost:27017").await {
        Ok(e) => e,
        Err(_e) => {
            error!("MongoDB client connection");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    info!("mongo client options success");

    let mongo_client = match MongoClient::with_options(mongo_client_options) {
        Ok(database) => database,
        Err(_e)  => {
            error!("MongoDB client option creation");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };

    info!("mongo client success");

    let off_database = mongo_client.database("off");
    let products_collection = off_database.collection::<OffProduct>("products");

    let mongo_filter = doc! {"ingredients_analysis_tags" : "en:vegan", "countries_tags.0" : "en:germany"};
    let mut results = match products_collection.find(mongo_filter).limit(1000).await {
        Ok(e) => e,
        Err(_e) => {
            error!("MongoDB Search failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };
    info!("mongo search success");

    meilisearch_client.delete_index("products").await.unwrap();

    let meilisearch_index = meilisearch_client.index("products");

    meilisearch_index.set_searchable_attributes(SEARCHABLE_ATTRIBUTES).await.unwrap();

    info!("meilisearch client success");

    while let Ok(Some(product)) = results.try_next().await {
        let vfb_product = product.to_vfb().await;
        match meilisearch_index.add_documents(&[vfb_product], Option::from("id")).await {
            Err(_e) => {
                {
                    error!("Meilisearch index failed");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            },
            _ => ()
        };
    };
    info!("Indexing complete");
    StatusCode::OK.into_response()
}

pub async fn search(search_query: Query<SearchQuery>) -> templates::ProductListTemplate {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));
    info!("start search");
    let mut products:Vec<templates::ProductListResult> = vec!();
    match meilisearch_client.index("products")
        .search()
        .with_query(&search_query.search_text)
        .with_show_matches_position(true)
        .execute::<VFBProduct>().await {
        Ok(e) => {
            for product in e.hits {
                products.push(templates::ProductListResult {
                    id : product.result.id,
                    name : {
                        if product.result.name_de.clone().is_some_and(|s| s.len() > 0) {
                            product.result.name_de
                        } else {
                            product.result.name_en
                        }
                    },
                    front_image: product.result.front_image,
                    matches_position : {
                        if let Some(matches) = product.matches_position {
                            matching_names(matches, &search_query.language)
                        } else {
                            let hash_map: HashMap<String, String> = HashMap::new();
                            hash_map
                        }
                    },
                    locale: search_query.language.clone()
                });
            }
        },
        Err(e) => {
            warn!("Unable to locate a razor: {e}, retrying");
            panic!("OH-NO")
        }
    };
    templates::ProductListTemplate {
        products: products,
        matches_with_text: t!( "matches_with" , locale = &search_query.language.clone()).to_string()
    }
}


pub async fn product(info_query: Query<InfoQuery>) -> impl IntoResponse {
    let meilisearch_client = Client::new("http://localhost:7700", Some("admin"));
    info!("start search");
    match meilisearch_client.index("products")
        .search()
        .with_query(&info_query.id)
        .execute::<templates::ProductInfoTemplate>().await {
        Ok(e) => {
            match e.hits.clone().into_iter().nth(0) {
                Some(x) => x.result.into_response(),
                None => {
                    error!("{:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        },
        Err(e) => {
            warn!("Unable to locate a razor: {e}, retrying");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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
        Value::Number(number) => match number.as_u64() {
            Some(x) => x.to_string(),
            None => return None,
        }.to_string(),
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

fn matching_names(matches: HashMap<String, Vec<MatchRange>>, locale: &String) -> HashMap<String, String> {
    let mut match_set: HashMap<String, String> = HashMap::new();
    let regex = Regex::new(r"(_en|_de)").unwrap();
    for matching in matches.into_iter() {
        let cropped_matching = regex.replace_all(&matching.0, "").to_string();
        match_set.insert(cropped_matching.clone(), t!(&cropped_matching, locale = locale).to_string());
    }
    match_set
}
