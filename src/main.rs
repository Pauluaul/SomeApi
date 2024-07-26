mod database;

use axum::{Router, routing::get};
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use mongodb::bson::doc;
use axum::extract::Path;
use log::LevelFilter;
use serde_json::Value;
use tower_http::services::ServeFile;
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
    simple_logging::log_to_file("log.log", LevelFilter::Info).expect("weird");
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
        .route("/trigger_delay", get(database::search))
        .route("/reindex", get(database::get_off_items))
        .route("/product", get(database::product))
        .route_service("/css", ServeFile::new("src/frontend/styles.css"))
}
async fn main_page(locale: Option<Path<templates::Locales>>) -> templates::HomeTemplate {
    let locale = locale.unwrap_or(Path(templates::Locales::DE)).0;
    templates::HomeTemplate {
        search: t!("search", locale = locale.to_string()).to_string(),
        locale: locale //no shorthand wegen übersicht
    }
}