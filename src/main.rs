use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json as AxumJson};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::{query_as, Error, FromRow, PgPool};
use std::env;
use tokio::net::TcpListener;

#[derive(Serialize, FromRow)]
struct Item {
    id: i32,
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct RequestItem {
    name: String,
    description: String,
}

#[derive(Serialize)]
struct DeletedItemsResponse {
    deleted_count: u64,
}

#[derive(Clone)]
struct AppState {
    db_pool: PgPool,
}

impl AppState {
    async fn create_item(&self, name: &str, description: &str) -> Result<Item, Error> {
        let query = r#"
            INSERT INTO items (name, description)
            VALUES ($1, $2)
            RETURNING id, name, description
        "#;
        let row: (i32, String, String) = query_as(query)
            .bind(name)
            .bind(description)
            .fetch_one(&self.db_pool)
            .await?;

        Ok(Item {
            id: row.0,
            name: row.1,
            description: row.2,
        })
    }

    async fn get_items(&self) -> Result<Vec<Item>, Error> {
        let query = r#"
            SELECT * FROM items
        "#;
        let result = query_as::<_, Item>(query).fetch_all(&self.db_pool).await?;

        Ok(result)
    }

    async fn get_item(&self, id: i32) -> Result<Option<Item>, Error> {
        let query = r#"
            SELECT * FROM items WHERE id = $1
        "#;
        let result = query_as::<_, Item>(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await?;

        Ok(result)
    }

    async fn update_item(
        &self,
        id: i32,
        name: &str,
        description: &str,
    ) -> Result<Option<Item>, Error> {
        let query = r#"
            UPDATE items
            SET name = $1, description = $2
            WHERE id = $3
            RETURNING id, name, description
        "#;
        let result = query_as::<_, Item>(query)
            .bind(name)
            .bind(description)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await?;

        Ok(result)
    }

    async fn delete_item(&self, id: i32) -> Result<bool, Error> {
        let query = r#"
            DELETE FROM items WHERE id = $1
        "#;
        let result = sqlx::query(query).bind(id).execute(&self.db_pool).await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_all_items(&self) -> Result<u64, Error> {
        let query = r#"
            DELETE FROM items
        "#;
        let result = sqlx::query(query).execute(&self.db_pool).await?;

        Ok(result.rows_affected())
    }
}

async fn root() -> &'static str {
    "Items API :)"
}

async fn create_item(
    State(state): State<AppState>,
    Json(payload): Json<RequestItem>,
) -> (StatusCode, AxumJson<Item>) {
    let item = state
        .create_item(&payload.name, &payload.description)
        .await
        .unwrap();
    (StatusCode::CREATED, AxumJson(item))
}

async fn get_items(State(state): State<AppState>) -> impl IntoResponse {
    AxumJson(state.get_items().await.unwrap())
}

async fn get_item(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> impl IntoResponse {
    match state.get_item(id).await.unwrap() {
        Some(item) => (StatusCode::OK, AxumJson(item)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn update_item(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<RequestItem>,
) -> impl IntoResponse {
    match state
        .update_item(id, &payload.name, &payload.description)
        .await
    {
        Ok(Some(item)) => (StatusCode::OK, AxumJson(item)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn delete_item(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> impl IntoResponse {
    match state.delete_item(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn delete_all_items(State(state): State<AppState>) -> impl IntoResponse {
    match state.delete_all_items().await {
        Ok(deleted_count) => (
            StatusCode::OK,
            AxumJson(DeletedItemsResponse { deleted_count }),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let db_pool = PgPool::connect(&database_url)
        .await
        .expect("Cannot connect to database");

    let app = Router::new()
        .route("/", get(root))
        .route(
            "/items",
            get(get_items).post(create_item).delete(delete_all_items),
        )
        .route(
            "/items/{id}",
            get(get_item).put(update_item).delete(delete_item),
        )
        .with_state(AppState { db_pool });

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
